use std::collections::HashMap;

use crate::shared::ids::PeerId;

use super::discovered_peer::DiscoveredPeer;

/// Registry in-memory de peers descobertos via mDNS.
///
/// Sem Arc/Mutex — o wrapping fica na camada de aplicação.
#[derive(Debug, Default)]
pub struct DiscoveryRegistry {
    peers: HashMap<PeerId, DiscoveredPeer>,
}

impl DiscoveryRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insere ou atualiza um peer. Se já existia, toca o `last_seen`.
    pub fn appeared(&mut self, peer: DiscoveredPeer) {
        self.peers
            .entry(peer.id)
            .and_modify(|existing| {
                existing.username.clone_from(&peer.username);
                existing.fingerprint.clone_from(&peer.fingerprint);
                existing.addresses.clone_from(&peer.addresses);
                existing.touch();
            })
            .or_insert(peer);
    }

    /// Remove um peer. Retorna o peer removido ou `None` se não existia.
    pub fn disappeared(&mut self, peer_id: PeerId) -> Option<DiscoveredPeer> {
        self.peers.remove(&peer_id)
    }

    pub fn peers(&self) -> Vec<&DiscoveredPeer> {
        self.peers.values().collect()
    }

    pub fn get(&self, id: PeerId) -> Option<&DiscoveredPeer> {
        self.peers.get(&id)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use uuid::Uuid;

    use super::*;

    fn peer(id: PeerId) -> DiscoveredPeer {
        DiscoveredPeer::new(
            id,
            "Test PC",
            "AA:BB:CC:DD:EE:FF:00:11",
            vec![SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
                7878,
            )],
        )
    }

    #[test]
    fn appeared_adds_peer() {
        let mut reg = DiscoveryRegistry::new();
        let id = PeerId::from_uuid(Uuid::new_v4());
        reg.appeared(peer(id));
        assert_eq!(reg.len(), 1);
        assert!(reg.get(id).is_some());
    }

    #[test]
    fn appeared_is_idempotent() {
        let mut reg = DiscoveryRegistry::new();
        let id = PeerId::from_uuid(Uuid::new_v4());
        reg.appeared(peer(id));
        reg.appeared(peer(id));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn appeared_updates_existing_peer() {
        let mut reg = DiscoveryRegistry::new();
        let id = PeerId::from_uuid(Uuid::new_v4());
        reg.appeared(peer(id));

        let updated = DiscoveredPeer::new(id, "Updated Name", "AA:BB:CC:DD:EE:FF:00:11", vec![]);
        reg.appeared(updated);

        assert_eq!(reg.get(id).unwrap().username, "Updated Name");
    }

    #[test]
    fn disappeared_removes_peer() {
        let mut reg = DiscoveryRegistry::new();
        let id = PeerId::from_uuid(Uuid::new_v4());
        reg.appeared(peer(id));
        let removed = reg.disappeared(id);
        assert!(removed.is_some());
        assert!(reg.is_empty());
    }

    #[test]
    fn disappeared_unknown_peer_returns_none() {
        let mut reg = DiscoveryRegistry::new();
        let id = PeerId::from_uuid(Uuid::new_v4());
        assert!(reg.disappeared(id).is_none());
    }
}
