use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::focus::FocusTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotkeyAction {
    PanicLocal,
    ToggleLock,
    ForceReset,
    OpenActiveWorkspace,
}

impl HotkeyAction {
    #[must_use]
    pub const fn force_reset() -> Self {
        HotkeyAction::ForceReset
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSwitched {
    pub from: FocusTarget,
    pub to: FocusTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyTriggered {
    pub action: HotkeyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBlocked {
    pub blocked: bool,
    pub peer_id: Option<PeerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMonitorsUpdated {
    pub peer_id: PeerId,
    pub monitor_count: usize,
}
