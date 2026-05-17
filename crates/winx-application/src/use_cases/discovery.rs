use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tokio::sync::Mutex;
use tracing::{debug, info};
use winx_domain::{
    discovery::{DiscoveredPeer, DiscoveryRegistry},
    identity::Device,
    shared::ids::PeerId,
    DomainEvent,
};

use crate::{
    ports::{
        discovery::{AnnounceInfo, DiscoveryAdapter, DiscoveryEvent, WINX_KVM_PORT},
        IdentityStore,
    },
    EventBus,
};

/// Peer visto no mDNS com flag de confiança persistida (`peers.toml`).
#[derive(Debug, Clone)]
pub struct EnrichedDiscoveredPeer {
    pub peer: DiscoveredPeer,
    pub is_paired: bool,
}

/// Orquestra announce + browsing mDNS e mantém o registry de peers.
pub struct DiscoveryService {
    adapter: Arc<dyn DiscoveryAdapter>,
    registry: Arc<Mutex<DiscoveryRegistry>>,
    bus: EventBus,
    own_peer_id: Arc<Mutex<Option<PeerId>>>,
    running: Arc<AtomicBool>,
}

impl DiscoveryService {
    pub fn new(
        adapter: Arc<dyn DiscoveryAdapter>,
        registry: Arc<Mutex<DiscoveryRegistry>>,
        bus: EventBus,
    ) -> Self {
        Self {
            adapter,
            registry,
            bus,
            own_peer_id: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Inicia announce + browsing para o device informado.
    ///
    /// Idempotente: chamadas subsequentes são no-op enquanto o serviço estiver rodando.
    pub async fn start_for_device(&self, device: &Device) -> anyhow::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            debug!("discovery já rodando — ignorando chamada duplicada");
            return Ok(());
        }

        *self.own_peer_id.lock().await = Some(PeerId::from_uuid(device.id.as_uuid()));

        let info = AnnounceInfo {
            peer_id: PeerId::from_uuid(device.id.as_uuid()),
            username: device.username.clone(),
            fingerprint: device.public_key.fingerprint().to_string(),
            pubkey_hex: hex::encode(device.public_key.as_bytes()),
            port: WINX_KVM_PORT,
        };

        self.adapter.announce(&info).await?;
        info!(peer_id = %info.peer_id, username = %info.username, "mDNS announce iniciado");

        let mut rx = self.adapter.start_browsing().await?;

        let registry = Arc::clone(&self.registry);
        let bus = self.bus.clone();
        let own_id = Arc::clone(&self.own_peer_id);

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let own = *own_id.lock().await;
                match event {
                    DiscoveryEvent::Found {
                        peer_id,
                        username,
                        fingerprint,
                        addresses,
                    } => {
                        if Some(peer_id) == own {
                            debug!(%peer_id, "ignorando self-discovery");
                            continue;
                        }
                        info!(%peer_id, %username, "peer apareceu na rede");
                        let peer = DiscoveredPeer::new(peer_id, &username, &fingerprint, addresses);
                        registry.lock().await.appeared(peer);
                        bus.publish(DomainEvent::PeerAppeared(
                            winx_domain::discovery::PeerAppeared {
                                peer_id,
                                username,
                                fingerprint,
                            },
                        ));
                    }
                    DiscoveryEvent::Lost { peer_id } => {
                        info!(%peer_id, "peer desapareceu da rede");
                        registry.lock().await.disappeared(peer_id);
                        bus.publish(DomainEvent::PeerDisappeared(
                            winx_domain::discovery::PeerDisappeared { peer_id },
                        ));
                    }
                    DiscoveryEvent::Updated {
                        peer_id,
                        username,
                        addresses,
                    } => {
                        debug!(%peer_id, %username, "peer atualizado");
                        let mut reg = registry.lock().await;
                        if let Some(existing) = reg.get(peer_id) {
                            let fingerprint = existing.fingerprint.clone();
                            let updated =
                                DiscoveredPeer::new(peer_id, &username, &fingerprint, addresses);
                            reg.appeared(updated);
                        }
                        bus.publish(DomainEvent::PeerUpdated(
                            winx_domain::discovery::PeerUpdated { peer_id, username },
                        ));
                    }
                }
            }
        });

        Ok(())
    }

    /// Re-publica o device na rede com dados atualizados (ex.: novo username).
    pub async fn reannounce(&self, info: &AnnounceInfo) -> anyhow::Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            debug!("discovery não iniciado — ignorando reannounce");
            return Ok(());
        }
        self.adapter.reannounce(info).await
    }

    /// Snapshot do registry atual.
    pub async fn get_peers(&self) -> Vec<DiscoveredPeer> {
        self.registry
            .lock()
            .await
            .peers()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Lista peers do mDNS marcando quais já estão em `peers.toml`.
    pub async fn list_peers_enriched(
        &self,
        identity: &dyn IdentityStore,
    ) -> anyhow::Result<Vec<EnrichedDiscoveredPeer>> {
        let trusted_ids: HashSet<PeerId> = identity
            .load_peers()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect();

        Ok(self
            .get_peers()
            .await
            .into_iter()
            .map(|peer| EnrichedDiscoveredPeer {
                is_paired: trusted_ids.contains(&peer.id),
                peer,
            })
            .collect())
    }

    /// Retorna o peer_id próprio (apenas se discovery foi iniciado).
    pub async fn get_own_peer_id(&self) -> Option<PeerId> {
        *self.own_peer_id.lock().await
    }

    /// Spawna uma task que monitora mudanças de rede e reanuncia mDNS.
    pub fn spawn_network_watcher<T: Send + 'static>(
        self: Arc<Self>,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<T>,
    ) {
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                if let Some(peer_id) = *self.own_peer_id.lock().await {
                    let info = AnnounceInfo {
                        peer_id,
                        username: String::new(),
                        fingerprint: String::new(),
                        pubkey_hex: String::new(),
                        port: WINX_KVM_PORT,
                    };
                    if let Err(e) = self.reannounce(&info).await {
                        debug!(?e, "reannounce falhou");
                    } else {
                        info!("mDNS reannounced após mudança de rede");
                    }
                }
            }
        });
    }

    /// Reconfigura as interfaces de rede para mDNS e reanuncia.
    pub async fn set_discovery_interfaces(&self, interfaces: &[String]) -> anyhow::Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            debug!("discovery não iniciado — ignorando set_discovery_interfaces");
            return Ok(());
        }

        info!("[DISCOVERY] reconfigurando interfaces: {:?}", interfaces);
        self.adapter.set_interfaces(interfaces).await?;

        if let Some(peer_id) = *self.own_peer_id.lock().await {
            let info = AnnounceInfo {
                peer_id,
                username: String::new(),
                fingerprint: String::new(),
                pubkey_hex: String::new(),
                port: WINX_KVM_PORT,
            };
            self.reannounce(&info).await?;
            info!("[DISCOVERY] mDNS reannounced após mudança de interfaces");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::Arc,
    };

    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use winx_domain::{
        discovery::{DiscoveredPeer, DiscoveryRegistry},
        identity::{PublicKey, TrustedPeer},
        shared::ids::PeerId,
    };

    use super::*;
    use crate::ports::IdentityStore;

    struct MockIdentityStore {
        peers: Vec<TrustedPeer>,
    }

    #[async_trait]
    impl IdentityStore for MockIdentityStore {
        async fn load_device(&self) -> anyhow::Result<Option<winx_domain::identity::Device>> {
            Ok(None)
        }

        async fn save_device(&self, _device: &winx_domain::identity::Device) -> anyhow::Result<()> {
            Ok(())
        }

        async fn load_peers(&self) -> anyhow::Result<Vec<TrustedPeer>> {
            Ok(self.peers.clone())
        }

        async fn save_peer(&self, _peer: &TrustedPeer) -> anyhow::Result<()> {
            Ok(())
        }

        async fn remove_peer(&self, _peer_id: PeerId) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopDiscoveryAdapter;

    #[async_trait]
    impl DiscoveryAdapter for NoopDiscoveryAdapter {
        async fn announce(&self, _info: &AnnounceInfo) -> anyhow::Result<()> {
            Ok(())
        }

        async fn reannounce(&self, _info: &AnnounceInfo) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop_announcing(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn start_browsing(
            &self,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<DiscoveryEvent>> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }

        async fn stop_browsing(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn set_interfaces(&self, _names: &[String]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn sample_peer(id: PeerId, name: &str) -> DiscoveredPeer {
        DiscoveredPeer::new(
            id,
            name,
            "AA:BB:CC:DD:EE:FF:00:11",
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878)],
        )
    }

    #[tokio::test]
    async fn enriched_marks_trusted_peer() {
        let peer_a = PeerId::from_uuid(Uuid::new_v4());
        let peer_b = PeerId::from_uuid(Uuid::new_v4());

        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        registry
            .lock()
            .await
            .appeared(sample_peer(peer_a, "A"));
        registry
            .lock()
            .await
            .appeared(sample_peer(peer_b, "B"));

        let svc = DiscoveryService::new(
            Arc::new(NoopDiscoveryAdapter),
            Arc::clone(&registry),
            EventBus::new(),
        );

        let store = MockIdentityStore {
            peers: vec![TrustedPeer::new(
                peer_a,
                "A",
                PublicKey::new([0x11; 32]),
            )],
        };

        let list = svc.list_peers_enriched(&store).await.unwrap();
        let a = list.iter().find(|p| p.peer.id == peer_a).unwrap();
        let b = list.iter().find(|p| p.peer.id == peer_b).unwrap();
        assert!(a.is_paired);
        assert!(!b.is_paired);
    }
}
