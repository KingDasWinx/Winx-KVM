use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::shared::ids::PeerId;

/// Peer visto na rede via mDNS — pode ou não ser confiável (ver `TrustedPeer`).
///
/// Não persiste: o registry é reconstruído a cada execução via mDNS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub id: PeerId,
    pub username: String,
    /// Fingerprint Ed25519 anunciado via TXT record (ex: `"A3:4F:B2:1C:D9:E5:07:AB"`).
    pub fingerprint: String,
    pub addresses: Vec<SocketAddr>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen: OffsetDateTime,
}

impl DiscoveredPeer {
    pub fn new(
        id: PeerId,
        username: impl Into<String>,
        fingerprint: impl Into<String>,
        addresses: Vec<SocketAddr>,
    ) -> Self {
        Self {
            id,
            username: username.into(),
            fingerprint: fingerprint.into(),
            addresses,
            last_seen: OffsetDateTime::now_utc(),
        }
    }

    /// Atualiza `last_seen` para agora.
    pub fn touch(&mut self) {
        self.last_seen = OffsetDateTime::now_utc();
    }

    pub fn update_addresses(&mut self, addrs: Vec<SocketAddr>) {
        self.addresses = addrs;
        self.touch();
    }
}
