use std::net::SocketAddr;

use async_trait::async_trait;
use tokio::sync::mpsc;
use winx_domain::shared::ids::PeerId;

/// Porta QUIC usada como placeholder até o Sprint 4.
pub const WINX_KVM_PORT: u16 = 7878;

/// Dados necessários para anunciar este device via mDNS.
#[derive(Debug, Clone)]
pub struct AnnounceInfo {
    pub peer_id: PeerId,
    pub username: String,
    pub fingerprint: String,
    pub port: u16,
}

/// Eventos gerados pelo adapter ao observar a rede.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    Found {
        peer_id: PeerId,
        username: String,
        fingerprint: String,
        addresses: Vec<SocketAddr>,
    },
    Lost {
        peer_id: PeerId,
    },
    Updated {
        peer_id: PeerId,
        username: String,
        addresses: Vec<SocketAddr>,
    },
}

/// Port de descoberta de peers na rede local.
///
/// Implementação concreta em `winx-infra::discovery_mdns`.
#[async_trait]
pub trait DiscoveryAdapter: Send + Sync + 'static {
    /// Publica este device na rede (mDNS announce).
    async fn announce(&self, info: &AnnounceInfo) -> anyhow::Result<()>;

    /// Atualiza o anúncio mDNS (unregister + register) sem derrubar o browsing.
    async fn reannounce(&self, info: &AnnounceInfo) -> anyhow::Result<()>;

    /// Remove o anúncio da rede.
    async fn stop_announcing(&self) -> anyhow::Result<()>;

    /// Inicia a escuta de peers na rede. Retorna um receiver de eventos.
    async fn start_browsing(&self) -> anyhow::Result<mpsc::Receiver<DiscoveryEvent>>;

    /// Para a escuta de peers.
    async fn stop_browsing(&self) -> anyhow::Result<()>;
}
