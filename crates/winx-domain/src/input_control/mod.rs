//! Bounded context: **InputControl**.
//!
//! Captura input local, decide para quem mandar (foco), injeta input vindo
//! do peer, gerencia layout de monitores e hotkeys.
//!
//! Ver [docs/PLANNING.md][p] épico 6.
//!
//! [p]: ../../../../../docs/PLANNING.md

pub mod events;
pub mod focus;
pub mod input_event;
pub mod layout;
pub mod monitor;

pub use events::{FocusSwitched, HotkeyAction, HotkeyTriggered, InputBlocked};
pub use focus::{FocusState, FocusTarget};
pub use input_event::{InputEvent, KeyModifiers, MouseButton, PortableKeyCode};
pub use layout::MonitorLayout;
pub use monitor::{MonitorId, MonitorRect};
