use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use winx_application::ports::DiscoveryQuery;
use winx_domain::{
    discovery::DiscoveryRegistry,
    shared::ids::{DeviceId, PeerId},
};

/// Resolve endereços de peers a partir do registry mDNS compartilhado.
///
/// Usado pelo `WorkspaceService` para enviar invites/sync/cursor — mesma fonte
/// que `PairingService::resolve_peer_addr`.
pub struct RegistryDiscoveryQuery {
    registry: Arc<Mutex<DiscoveryRegistry>>,
}

impl RegistryDiscoveryQuery {
    pub fn new(registry: Arc<Mutex<DiscoveryRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl DiscoveryQuery for RegistryDiscoveryQuery {
    async fn resolve_address(&self, device_id: DeviceId) -> anyhow::Result<Option<SocketAddr>> {
        let peer_id = PeerId::from_uuid(device_id.as_uuid());
        let reg = self.registry.lock().await;
        Ok(reg
            .get(peer_id)
            .and_then(|peer| peer.addresses.first().copied()))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use uuid::Uuid;
    use winx_domain::discovery::DiscoveredPeer;

    use super::*;

    #[tokio::test]
    async fn resolves_first_address_from_registry() {
        let id = PeerId::from_uuid(Uuid::new_v4());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8)), 7878);
        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        registry.lock().await.appeared(DiscoveredPeer::new(
            id,
            "remote",
            "AA:BB:CC:DD:EE:FF:00:11",
            vec![addr],
        ));

        let query = RegistryDiscoveryQuery::new(registry);
        let device_id = DeviceId::from_uuid(id.as_uuid());
        let resolved = query.resolve_address(device_id).await.unwrap();
        assert_eq!(resolved, Some(addr));
    }

    #[tokio::test]
    async fn returns_none_for_unknown_device() {
        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        let query = RegistryDiscoveryQuery::new(registry);
        let device_id = DeviceId::from_uuid(Uuid::new_v4());
        assert!(query.resolve_address(device_id).await.unwrap().is_none());
    }
}
