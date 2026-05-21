//! Máquina de estados de foco (sem async, sem Mutex).

use super::focus::{FocusState, FocusTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusTransition {
    pub from: FocusTarget,
    pub to: FocusTarget,
    pub lock_mode_after: bool,
}

#[must_use]
pub fn apply_focus_target(state: &mut FocusState, to: FocusTarget) -> FocusTransition {
    let from = state.target.clone();
    state.target = to;
    FocusTransition {
        from,
        to: state.target.clone(),
        lock_mode_after: state.lock_mode,
    }
}

#[must_use]
pub fn toggle_lock_mode(state: &mut FocusState) -> bool {
    state.lock_mode = !state.lock_mode;
    state.lock_mode
}

#[must_use]
pub fn force_local(state: &mut FocusState) -> FocusTransition {
    state.lock_mode = false;
    apply_focus_target(state, FocusTarget::Local)
}

#[must_use]
pub fn input_should_be_swallowed(state: &FocusState) -> bool {
    matches!(state.target, FocusTarget::Remote(_)) && !state.lock_mode
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::PeerId;
    use uuid::Uuid;

    #[test]
    fn default_is_local_unlocked() {
        let s = FocusState::default();
        assert!(!input_should_be_swallowed(&s));
    }

    #[test]
    fn remote_swallows_when_not_locked() {
        let mut s = FocusState::default();
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let _ = apply_focus_target(&mut s, FocusTarget::Remote(peer));
        assert!(input_should_be_swallowed(&s));
    }

    #[test]
    fn lock_mode_blocks_swallow_semantics_for_edge_only() {
        let mut s = FocusState {
            lock_mode: true,
            ..FocusState::default()
        };
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let _ = apply_focus_target(&mut s, FocusTarget::Remote(peer));
        assert!(!input_should_be_swallowed(&s));
    }

    #[test]
    fn toggle_lock_flips_flag() {
        let mut s = FocusState::default();
        assert!(toggle_lock_mode(&mut s));
        assert!(s.lock_mode);
        assert!(!toggle_lock_mode(&mut s));
    }

    #[test]
    fn force_local_clears_lock() {
        let mut s = FocusState {
            lock_mode: true,
            ..FocusState::default()
        };
        let t = force_local(&mut s);
        assert!(matches!(t.to, FocusTarget::Local));
        assert!(!s.lock_mode);
    }
}
