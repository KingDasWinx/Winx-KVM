//! Mapeamento VK Win32 ↔ códigos portáveis (scan code).

use winx_domain::input_control::{MouseButton, PortableKeyCode};

/// Botão do mouse a partir do flag Win32 (`WM_*BUTTON*` / hook flags).
#[must_use]
pub fn mouse_button_from_hook_flag(flag: u32) -> Option<MouseButton> {
    const LLMHF: u32 = 0x0000_0001; // placeholder - actual flags from win32
    let _ = LLMHF;
    match flag {
        0x0001 => Some(MouseButton::Left),
        0x0010 => Some(MouseButton::Right),
        0x0020 => Some(MouseButton::Middle),
        0x0002 => Some(MouseButton::X1),
        0x0004 => Some(MouseButton::X2),
        _ => None,
    }
}

/// Converte VK Win32 para código portável (scan code).
#[must_use]
pub fn vk_to_portable(vk: u16) -> Option<PortableKeyCode> {
    // MapVirtualKeyW VK -> scancode seria ideal; v0.1: VK como portable para teclas comuns
    if vk == 0 {
        return None;
    }
    Some(PortableKeyCode(vk))
}

#[must_use]
pub fn portable_to_vk(code: PortableKeyCode) -> u16 {
    code.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_key() {
        assert_eq!(vk_to_portable(0x41), Some(PortableKeyCode(0x41)));
    }
}
