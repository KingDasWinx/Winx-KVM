use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::connection::ConnectionStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEstablished {
    pub peer_id: PeerId,
    pub peer_username: String,
    /// `true` quando o peer remoto iniciou o QUIC (este device aceitou inbound).
    pub is_inbound: bool,
    /// Some quando a conexão pertence a um workspace, não ao modo single.
    pub via_workspace_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionLost {
    pub peer_id: PeerId,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsUpdated {
    pub peer_id: PeerId,
    pub stats: ConnectionStats,
}
