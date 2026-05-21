//! Bounded context: **InputControl**.
//!
//! Captura input local, decide para quem mandar (foco), injeta input vindo
//! do peer, gerencia layout de monitores e hotkeys.
//!
//! Ver [docs/PLANNING.md][p] épico 6.
//!
//! [p]: ../../../../../docs/PLANNING.md

pub mod edge;
pub mod events;
pub mod focus;
pub mod focus_policy;
pub mod input_event;
pub mod layout;
pub mod monitor;

pub use edge::{
    accumulate_return_left, should_switch_to_remote, EdgeDetectInput, EDGE_TOLERANCE_PX,
    RETURN_LEFT_THRESHOLD_PX,
};
pub use events::{FocusSwitched, HotkeyAction, HotkeyTriggered, InputBlocked};
pub use focus::{FocusState, FocusTarget};
pub use focus_policy::{
    apply_focus_target, force_local, input_should_be_swallowed, toggle_lock_mode, FocusTransition,
};
pub use input_event::{InputEvent, KeyModifiers, MouseButton, PortableKeyCode};
pub use layout::MonitorLayout;
pub use monitor::{MonitorId, MonitorRect};
