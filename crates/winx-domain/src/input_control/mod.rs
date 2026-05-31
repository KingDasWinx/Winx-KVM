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
pub mod mouse;

pub use edge::{
    should_return_to_local, should_switch_to_remote, EdgeDetectInput, RemoteCursorEst,
    EDGE_TOLERANCE_PX, REMOTE_ENTRY_INSET_PX,
};
pub use events::{FocusSwitched, HotkeyAction, HotkeyTriggered, InputBlocked, PeerMonitorsUpdated};
pub use focus::{FocusState, FocusTarget};
pub use focus_policy::{
    apply_focus_target, force_local, input_should_be_swallowed, toggle_lock_mode, FocusTransition,
};
pub use input_event::{InputEvent, KeyModifiers, MouseButton, PortableKeyCode};
pub use layout::{BorderSide, EdgeConfig, MonitorLayout};
pub use monitor::{MonitorId, MonitorRect};
pub use mouse::{MOUSE_COALESCE_FLUSH_MS, MOUSE_SEND_MIN_MANHATTAN};
