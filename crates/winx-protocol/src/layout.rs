//! Layout KVM / monitores (sync single connection via stream Data).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::workspace::{EdgeConfigPayload, MonitorLayoutPayload, MonitorRectPayload};

/// Anuncia monitores locais do remetente.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerMonitorsPayload {
    pub monitors: Vec<MonitorRectPayload>,
}

/// Layout canônico compartilhado da sessão (mesmas coords em todos os PCs).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct KvmSessionLayoutPayload {
    pub per_device: BTreeMap<String, Vec<MonitorRectPayload>>,
}
