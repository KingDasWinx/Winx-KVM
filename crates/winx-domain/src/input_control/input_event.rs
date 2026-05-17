use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Código de tecla portável (scan code USB HID / valor estável no wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortableKeyCode(pub u16);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InputEvent {
    MouseMove {
        dx: i32,
        dy: i32,
        /// Posição absoluta na área virtual do Windows (detecção de borda local).
        screen_x: i32,
        screen_y: i32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    MouseScroll {
        delta_x: i32,
        delta_y: i32,
    },
    Key {
        code: PortableKeyCode,
        pressed: bool,
        modifiers: KeyModifiers,
    },
}

impl InputEvent {
    #[must_use]
    pub fn delta(&self) -> (i32, i32) {
        match self {
            Self::MouseMove { dx, dy, .. } => (*dx, *dy),
            _ => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_move_carries_signed_deltas() {
        let e = InputEvent::MouseMove {
            dx: -3,
            dy: 10,
            screen_x: 100,
            screen_y: 200,
        };
        assert_eq!(e.delta(), (-3, 10));
    }
}
