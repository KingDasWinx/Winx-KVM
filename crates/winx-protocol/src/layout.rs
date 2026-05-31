//! Layout KVM / monitores (sync single connection via stream Data).

use serde::{Deserialize, Serialize};

pub use crate::workspace::{EdgeConfigPayload, MonitorLayoutPayload, MonitorRectPayload};

/// Anuncia monitores locais do remetente.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerMonitorsPayload {
    pub monitors: Vec<MonitorRectPayload>,
}
