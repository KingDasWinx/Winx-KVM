use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::connection::ConnectionStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEstablished {
    pub peer_id: PeerId,
    pub peer_username: String,
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
