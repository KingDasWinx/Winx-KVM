use async_trait::async_trait;
use std::net::SocketAddr;
use winx_domain::shared::ids::DeviceId;

/// Port simples para resolver SocketAddr de um device descoberto.
///
/// Usado para enviar invites e mensagens peer-to-peer. Implementação
/// pode ser em-memory cache atualizado pelo DiscoveryAdapter.
#[async_trait]
pub trait DiscoveryQuery: Send + Sync + 'static {
    /// Resolve o endereço de rede de um device. Retorna `None` se offline.
    async fn resolve_address(&self, device_id: DeviceId) -> anyhow::Result<Option<SocketAddr>>;
}
