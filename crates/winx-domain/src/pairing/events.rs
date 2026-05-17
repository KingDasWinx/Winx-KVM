use serde::{Deserialize, Serialize};

use crate::shared::{
    ids::{PeerId, SessionId},
    DomainErrorCode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequested {
    pub session_id: SessionId,
    pub peer_id: PeerId,
    pub peer_username: String,
    pub pin: String,
}

/// Pedido de pareamento recebido da rede (lado responder).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingIncoming {
    pub session_id: SessionId,
    pub peer_id: PeerId,
    pub peer_username: String,
    pub initiator_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingCompleted {
    pub session_id: SessionId,
    pub peer_id: PeerId,
    pub peer_username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingCancelled {
    pub session_id: SessionId,
    pub peer_id: PeerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingFailed {
    pub session_id: SessionId,
    pub peer_id: PeerId,
    pub reason: DomainErrorCode,
}
