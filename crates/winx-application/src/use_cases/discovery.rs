use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
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
    ports::discovery::{AnnounceInfo, DiscoveryAdapter, DiscoveryEvent, WINX_KVM_PORT},
    EventBus,
};

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
            while let Some(_) = rx.recv().await {
                if let Some(peer_id) = *self.own_peer_id.lock().await {
                    let info = AnnounceInfo {
                        peer_id,
                        username: "".to_string(),
                        fingerprint: "".to_string(),
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
                username: "".to_string(),
                fingerprint: "".to_string(),
                port: WINX_KVM_PORT,
            };
            self.reannounce(&info).await?;
            info!("[DISCOVERY] mDNS reannounced após mudança de interfaces");
        }

        Ok(())
    }
}
