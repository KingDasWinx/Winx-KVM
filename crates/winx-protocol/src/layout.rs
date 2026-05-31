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

/// Posição do cursor unificado da sessão KVM (stream Data).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionCursorSyncPayload {
    pub device_id: uuid::Uuid,
    pub x: i32,
    pub y: i32,
    pub seq: u64,
}

/// Peer remoto retoma controle com mouse físico local.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionInputTakeoverPayload {
    pub device_id: uuid::Uuid,
    pub x: i32,
    pub y: i32,
    pub seq: u64,
}
