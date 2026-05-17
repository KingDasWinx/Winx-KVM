use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

/// Peer novo detectado na rede via mDNS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAppeared {
    pub peer_id: PeerId,
    pub username: String,
    pub fingerprint: String,
}

/// Peer deixou de anunciar-se na rede.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDisappeared {
    pub peer_id: PeerId,
}

/// Dados de um peer já conhecido foram atualizados (username ou endereços).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerUpdated {
    pub peer_id: PeerId,
    pub username: String,
}
