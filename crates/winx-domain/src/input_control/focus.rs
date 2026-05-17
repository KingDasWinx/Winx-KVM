use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusTarget {
    Local,
    Remote(PeerId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusState {
    pub target: FocusTarget,
    /// Scroll Lock: impede troca de foco ao cruzar borda.
    pub lock_mode: bool,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            target: FocusTarget::Local,
            lock_mode: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_focus_is_local() {
        let f = FocusState::default();
        assert!(matches!(f.target, FocusTarget::Local));
        assert!(!f.lock_mode);
    }
}
