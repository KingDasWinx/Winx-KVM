//! Payload de input (mouse/teclado) no stream QUIC Input.

use serde::{Deserialize, Serialize};

/// Evento portável no wire (espelha `winx_domain::input_control::InputEvent`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InputEventDto {
    MouseMove {
        dx: i32,
        dy: i32,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
    MouseScroll {
        delta_x: i32,
        delta_y: i32,
    },
    Key {
        code: u16,
        pressed: bool,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    },
    /// Teleporte absoluto do cursor para as coordenadas de tela do receiver.
    MouseWarpAbsolute {
        x: i32,
        y: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputPayload {
    pub seq: u64,
    pub event: InputEventDto,
}
