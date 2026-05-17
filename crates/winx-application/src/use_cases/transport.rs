use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use tokio::sync::Mutex;
use tracing::{info, warn};
use winx_domain::{
    discovery::DiscoveryRegistry,
    shared::{ids::PeerId, DomainErrorCode},
    transport::{
        events::{ConnectionEstablished, ConnectionLost, StatsUpdated},
        Connection, ConnectionStats,
    },
    DomainError, DomainEvent,
};

use crate::{
    bus::EventBus,
    ports::{
        transport::{IncomingConnection, StreamReceiver, StreamSender, TransportAdapter},
        IdentityStore,
    },
};
use winx_domain::transport::StreamKind;

const MAX_RECONNECT_ATTEMPTS: u32 = 3;
const STATS_RTT_DELTA_MS: u32 = 5;

pub struct TransportService {
    adapter: Arc<dyn TransportAdapter>,
    connections: Arc<Mutex<HashMap<PeerId, Connection>>>,
    identity_store: Arc<dyn IdentityStore>,
    discovery_registry: Arc<Mutex<DiscoveryRegistry>>,
    bus: EventBus,
}

impl TransportService {
    pub fn new(
        adapter: Arc<dyn TransportAdapter>,
        identity_store: Arc<dyn IdentityStore>,
        discovery_registry: Arc<Mutex<DiscoveryRegistry>>,
        bus: EventBus,
    ) -> Self {
        Self {
            adapter,
            connections: Arc::new(Mutex::new(HashMap::new())),
            identity_store,
            discovery_registry,
            bus,
        }
    }

    /// Inicia listener QUIC e tasks de manutenção.
    pub async fn start(&self, port: u16) -> anyhow::Result<()> {
        let mut incoming = self.adapter.listen(port).await?;

        let connections = Arc::clone(&self.connections);
        let identity_store = Arc::clone(&self.identity_store);
        let bus = self.bus.clone();

        tokio::spawn(async move {
            while let Some(conn) = incoming.recv().await {
                if let Err(err) = handle_incoming(conn, &connections, &identity_store, &bus).await {
                    warn!(?err, "falha ao aceitar conexão entrante");
                }
            }
        });

        self.spawn_bus_subscriber();
        self.spawn_maintenance_tasks();

        Ok(())
    }

    /// Conecta a um peer pareado. No-op se já conectado.
    pub async fn connect_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
        {
            let conns = self.connections.lock().await;
            if let Some(c) = conns.get(&peer_id) {
                if matches!(
                    c.state,
                    winx_domain::transport::ConnectionState::Connected { .. }
                ) {
                    return Ok(());
                }
            }
        }

        let trusted = self
            .identity_store
            .load_peers()
            .await
            .map_err(|e| internal_err(&e.to_string()))?
            .into_iter()
            .find(|p| p.id == peer_id)
            .ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::TransportPeerNotTrusted,
                    "peer não está na lista de confiança",
                )
            })?;

        let peer_addr = self.resolve_peer_addr(peer_id).await.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "endereço do peer não encontrado na rede",
            )
        })?;

        let mut entry = self.connections.lock().await;
        let conn = entry
            .entry(peer_id)
            .or_insert_with(|| Connection::new(peer_id));

        let active = self
            .adapter
            .connect(peer_addr, *trusted.public_key.as_bytes())
            .await
            .map_err(|e| {
                conn.mark_disconnected();
                DomainError::new(DomainErrorCode::TransportConnectionFailed, e.to_string())
            })?;

        conn.id = active.conn_id;
        conn.mark_connected();
        drop(entry);

        self.bus
            .publish(DomainEvent::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                peer_username: trusted.username.clone(),
            }));

        info!(%peer_id, %peer_addr, "conexão QUIC estabelecida");
        Ok(())
    }

    pub async fn disconnect_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
        let conn_id = {
            let mut conns = self.connections.lock().await;
            let conn = conns.get_mut(&peer_id).ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::TransportConnectionFailed,
                    "nenhuma conexão ativa para este peer",
                )
            })?;
            let id = conn.id;
            conn.mark_disconnected();
            id
        };

        self.adapter
            .close(conn_id, "disconnect solicitado")
            .await
            .map_err(|e| internal_err(&e.to_string()))?;

        self.bus
            .publish(DomainEvent::ConnectionLost(ConnectionLost {
                peer_id,
                reason: "disconnect solicitado".to_string(),
            }));

        Ok(())
    }

    pub async fn get_stats(&self, peer_id: PeerId) -> Result<ConnectionStats, DomainError> {
        let conn_id = {
            let conns = self.connections.lock().await;
            let conn = conns.get(&peer_id).ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::TransportConnectionFailed,
                    "nenhuma conexão ativa para este peer",
                )
            })?;
            conn.id
        };

        self.adapter
            .get_stats(conn_id)
            .await
            .map_err(|e| internal_err(&e.to_string()))
    }

    pub async fn is_peer_connected(&self, peer_id: PeerId) -> bool {
        self.connections
            .lock()
            .await
            .get(&peer_id)
            .is_some_and(winx_domain::transport::Connection::is_active)
    }

    /// Abre stream QUIC para um peer conectado.
    pub async fn open_stream_for_peer(
        &self,
        peer_id: PeerId,
        kind: StreamKind,
    ) -> Result<(StreamSender, StreamReceiver), DomainError> {
        let conn_id = {
            let conns = self.connections.lock().await;
            let conn = conns.get(&peer_id).ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::TransportConnectionFailed,
                    "nenhuma conexão ativa para este peer",
                )
            })?;
            conn.id
        };

        self.adapter
            .open_stream(conn_id, kind)
            .await
            .map_err(|e| internal_err(&e.to_string()))
    }

    pub async fn list_connections(&self) -> Vec<(PeerId, winx_domain::transport::ConnectionState)> {
        self.connections
            .lock()
            .await
            .iter()
            .map(|(id, c)| (*id, c.state.clone()))
            .collect()
    }

    #[allow(clippy::too_many_lines)]
    pub fn spawn_maintenance_tasks(&self) {
        let adapter = Arc::clone(&self.adapter);
        let connections = Arc::clone(&self.connections);
        let bus = self.bus.clone();

        // Stats ~1 Hz
        tokio::spawn(async move {
            let mut last_rtt: HashMap<PeerId, u32> = HashMap::new();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let snapshot: Vec<(PeerId, winx_domain::shared::ids::SessionId)> = connections
                    .lock()
                    .await
                    .iter()
                    .filter(|(_, c)| c.is_active())
                    .map(|(pid, c)| (*pid, c.id))
                    .collect();

                for (peer_id, conn_id) in snapshot {
                    let Ok(stats) = adapter.get_stats(conn_id).await else {
                        continue;
                    };
                    {
                        let mut conns = connections.lock().await;
                        if let Some(c) = conns.get_mut(&peer_id) {
                            c.update_stats(stats);
                        }
                    }
                    let publish = match last_rtt.get(&peer_id) {
                        None => true,
                        Some(prev) => stats.rtt_ms.abs_diff(*prev) > STATS_RTT_DELTA_MS,
                    };
                    if publish {
                        last_rtt.insert(peer_id, stats.rtt_ms);
                        bus.publish(DomainEvent::StatsUpdated(StatsUpdated { peer_id, stats }));
                    }
                }
            }
        });

        // Reconexão a cada 5s
        let this_connections = Arc::clone(&self.connections);
        let this_adapter = Arc::clone(&self.adapter);
        let this_discovery = Arc::clone(&self.discovery_registry);
        let this_identity = Arc::clone(&self.identity_store);
        let this_bus = self.bus.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let peers_to_retry: Vec<PeerId> = this_connections
                    .lock()
                    .await
                    .iter()
                    .filter(|(_, c)| {
                        matches!(
                            c.state,
                            winx_domain::transport::ConnectionState::Reconnecting { .. }
                        )
                    })
                    .map(|(id, _)| *id)
                    .collect();

                for peer_id in peers_to_retry {
                    let Some(addr) = resolve_addr(&this_discovery, peer_id).await else {
                        continue;
                    };
                    let Some(trusted) = load_trusted(&this_identity, peer_id).await else {
                        continue;
                    };

                    let conn_id = {
                        let mut conns = this_connections.lock().await;
                        let Some(c) = conns.get_mut(&peer_id) else {
                            continue;
                        };
                        if let winx_domain::transport::ConnectionState::Reconnecting { attempts } =
                            c.state
                        {
                            if attempts >= MAX_RECONNECT_ATTEMPTS {
                                c.mark_disconnected();
                                this_bus.publish(DomainEvent::ConnectionLost(ConnectionLost {
                                    peer_id,
                                    reason: "máximo de tentativas de reconexão".to_string(),
                                }));
                                continue;
                            }
                        }
                        c.id
                    };

                    match this_adapter
                        .connect(addr, *trusted.public_key.as_bytes())
                        .await
                    {
                        Ok(active) => {
                            let username = trusted.username.clone();
                            {
                                let mut conns = this_connections.lock().await;
                                if let Some(c) = conns.get_mut(&peer_id) {
                                    c.id = active.conn_id;
                                    c.mark_connected();
                                }
                            }
                            let _ = this_adapter
                                .close(conn_id, "substituída por reconexão")
                                .await;
                            this_bus.publish(DomainEvent::ConnectionEstablished(
                                ConnectionEstablished {
                                    peer_id,
                                    peer_username: username,
                                },
                            ));
                        }
                        Err(err) => {
                            warn!(%peer_id, ?err, "reconexão falhou");
                            let mut conns = this_connections.lock().await;
                            if let Some(c) = conns.get_mut(&peer_id) {
                                c.mark_reconnecting();
                            }
                        }
                    }
                }
            }
        });
    }

    fn spawn_bus_subscriber(&self) {
        let adapter = Arc::clone(&self.adapter);
        let identity_store = Arc::clone(&self.identity_store);
        let mut rx = self.bus.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let DomainEvent::PairingCompleted(e) = event {
                    if let Ok(peers) = identity_store.load_peers().await {
                        if let Some(peer) = peers.iter().find(|p| p.id == e.peer_id) {
                            adapter.add_trusted_key(*peer.public_key.as_bytes()).await;
                        }
                    }
                }
            }
        });
    }

    async fn resolve_peer_addr(&self, peer_id: PeerId) -> Option<SocketAddr> {
        resolve_addr(&self.discovery_registry, peer_id).await
    }
}

async fn resolve_addr(
    registry: &Arc<Mutex<DiscoveryRegistry>>,
    peer_id: PeerId,
) -> Option<SocketAddr> {
    registry
        .lock()
        .await
        .get(peer_id)
        .and_then(|p| p.addresses.first().copied())
}

async fn load_trusted(
    store: &Arc<dyn IdentityStore>,
    peer_id: PeerId,
) -> Option<winx_domain::identity::TrustedPeer> {
    store
        .load_peers()
        .await
        .ok()?
        .into_iter()
        .find(|p| p.id == peer_id)
}

async fn handle_incoming(
    incoming: IncomingConnection,
    connections: &Arc<Mutex<HashMap<PeerId, Connection>>>,
    identity_store: &Arc<dyn IdentityStore>,
    bus: &EventBus,
) -> anyhow::Result<()> {
    let trusted = identity_store
        .load_peers()
        .await?
        .into_iter()
        .find(|p| p.public_key.as_bytes() == &incoming.peer_public_key);

    let Some(trusted) = trusted else {
        anyhow::bail!("conexão de peer não confiável");
    };

    let peer_id = trusted.id;
    let mut conns = connections.lock().await;
    let conn = conns
        .entry(peer_id)
        .or_insert_with(|| Connection::new(peer_id));
    conn.id = incoming.conn_id;
    conn.mark_connected();
    drop(conns);

    bus.publish(DomainEvent::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        peer_username: trusted.username.clone(),
    }));

    info!(%peer_id, "conexão entrante aceita");
    Ok(())
}

fn internal_err(msg: &str) -> DomainError {
    DomainError::new(DomainErrorCode::InternalError, msg)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::sync::{mpsc, Mutex};
    use uuid::Uuid;

    use winx_domain::{
        discovery::{DiscoveredPeer, DiscoveryRegistry},
        identity::{PublicKey, TrustedPeer},
        shared::ids::{PeerId, SessionId},
        transport::StreamKind,
    };

    use super::*;
    use crate::ports::transport::{ActiveConnection, IncomingConnection, TransportAdapter};

    struct MockTransportAdapter;

    #[async_trait]
    impl TransportAdapter for MockTransportAdapter {
        async fn listen(&self, _port: u16) -> anyhow::Result<mpsc::Receiver<IncomingConnection>> {
            let (_tx, rx) = mpsc::channel(1);
            Ok(rx)
        }

        async fn connect(
            &self,
            _peer_addr: SocketAddr,
            _peer_public_key: [u8; 32],
        ) -> anyhow::Result<ActiveConnection> {
            Ok(ActiveConnection {
                conn_id: SessionId::new(),
            })
        }

        async fn open_stream(
            &self,
            _conn_id: SessionId,
            _kind: StreamKind,
        ) -> anyhow::Result<(
            crate::ports::transport::StreamSender,
            crate::ports::transport::StreamReceiver,
        )> {
            let (tx, rx) = mpsc::channel(8);
            Ok((tx, rx))
        }

        async fn get_stats(&self, _conn_id: SessionId) -> anyhow::Result<ConnectionStats> {
            Ok(ConnectionStats {
                rtt_ms: 10,
                tx_bytes: 1,
                rx_bytes: 2,
                lost_packets: 0,
            })
        }

        async fn close(&self, _conn_id: SessionId, _reason: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn add_trusted_key(&self, _key: [u8; 32]) {}
    }

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

    async fn seed_discovered(registry: &Arc<Mutex<DiscoveryRegistry>>, peer_id: PeerId) {
        registry.lock().await.appeared(DiscoveredPeer::new(
            peer_id,
            "Peer",
            "AA:BB:CC:DD:EE:FF:00:11",
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878)],
        ));
    }

    #[tokio::test]
    async fn connect_peer_emits_connection_established() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let key = PublicKey::new([0x11; 32]);
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let adapter = Arc::new(MockTransportAdapter);
        let identity = Arc::new(MockIdentityStore {
            peers: vec![TrustedPeer::new(peer_id, "Peer A", key)],
        });
        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        seed_discovered(&registry, peer_id).await;

        let service = TransportService::new(adapter, identity, registry, bus);
        service.connect_peer(peer_id).await.unwrap();

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            DomainEvent::ConnectionEstablished(e) if e.peer_id == peer_id
        ));
    }

    #[tokio::test]
    async fn disconnect_peer_emits_connection_lost() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let key = PublicKey::new([0x22; 32]);
        let bus = EventBus::new();
        let adapter = Arc::new(MockTransportAdapter);
        let identity = Arc::new(MockIdentityStore {
            peers: vec![TrustedPeer::new(peer_id, "Peer B", key)],
        });
        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        seed_discovered(&registry, peer_id).await;
        let service = TransportService::new(adapter, identity, registry, bus.clone());
        let mut rx = bus.subscribe();

        service.connect_peer(peer_id).await.unwrap();
        let _ = rx.recv().await;

        service.disconnect_peer(peer_id).await.unwrap();
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DomainEvent::ConnectionLost(_)));
    }
}
