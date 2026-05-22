use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use winx_application::ports::DiscoveryQuery;
use winx_domain::shared::ids::DeviceId;

/// Implementação em-memory de DiscoveryQuery para testes e MVP.
///
/// Armazena um mapa DeviceId → SocketAddr que pode ser manipulado
/// externamente (em testes ou integrado com DiscoveryAdapter em W3).
pub struct MemoryDiscoveryQuery {
    cache: Arc<RwLock<HashMap<DeviceId, SocketAddr>>>,
}

impl MemoryDiscoveryQuery {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adiciona um endereço ao cache (usado em testes).
    pub async fn register(&self, device_id: DeviceId, addr: SocketAddr) {
        let mut cache = self.cache.write().await;
        cache.insert(device_id, addr);
    }
}

impl Default for MemoryDiscoveryQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoveryQuery for MemoryDiscoveryQuery {
    async fn resolve_address(&self, device_id: DeviceId) -> anyhow::Result<Option<SocketAddr>> {
        let cache = self.cache.read().await;
        Ok(cache.get(&device_id).copied())
    }
}
