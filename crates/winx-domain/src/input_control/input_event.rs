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
    /// Teleporte absoluto do cursor para (x, y) em coordenadas de tela do receiver.
    /// Enviado como primeiro frame ao entrar em foco remoto para preservar posição Y.
    MouseWarpAbsolute {
        x: i32,
        y: i32,
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

    /// `WM_MOUSEMOVE` sem deslocamento (baseline pós-warp, posição repetida).
    #[must_use]
    pub fn is_noop_mouse_move(&self) -> bool {
        matches!(self, Self::MouseMove { dx: 0, dy: 0, .. })
    }

    /// Movimento abaixo do limiar Manhattan (`|dx|+|dy| < min_manhattan`).
    #[must_use]
    pub fn is_noise_mouse_move(&self, min_manhattan: i32) -> bool {
        match self {
            Self::MouseMove { dx, dy, .. } => dx.abs().saturating_add(dy.abs()) < min_manhattan,
            _ => false,
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

    #[test]
    fn noop_mouse_move_detected() {
        let e = InputEvent::MouseMove {
            dx: 0,
            dy: 0,
            screen_x: 100,
            screen_y: 200,
        };
        assert!(e.is_noop_mouse_move());
        let e = InputEvent::MouseMove {
            dx: 1,
            dy: 0,
            screen_x: 100,
            screen_y: 200,
        };
        assert!(!e.is_noop_mouse_move());
    }

    #[test]
    fn mouse_move_below_threshold_is_noise() {
        let e = InputEvent::MouseMove {
            dx: 0,
            dy: 1,
            screen_x: 0,
            screen_y: 0,
        };
        assert!(e.is_noise_mouse_move(2));
        let e = InputEvent::MouseMove {
            dx: 2,
            dy: 2,
            screen_x: 0,
            screen_y: 0,
        };
        assert!(!e.is_noise_mouse_move(2));
    }
}
