//! Conversão entre tipos de domínio e wire format.

use winx_domain::input_control::{InputEvent, MouseButton, PortableKeyCode};
use winx_protocol::{InputEventDto, InputPayload};

pub fn input_event_to_dto(event: &InputEvent) -> InputEventDto {
    match event {
        InputEvent::MouseMove { dx, dy } => InputEventDto::MouseMove { dx: *dx, dy: *dy },
        InputEvent::MouseButton { button, pressed } => InputEventDto::MouseButton {
            button: mouse_button_to_u8(*button),
            pressed: *pressed,
        },
        InputEvent::MouseScroll { delta_x, delta_y } => InputEventDto::MouseScroll {
            delta_x: *delta_x,
            delta_y: *delta_y,
        },
        InputEvent::Key {
            code,
            pressed,
            modifiers,
        } => InputEventDto::Key {
            code: code.0,
            pressed: *pressed,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            meta: modifiers.meta,
        },
        _ => InputEventDto::MouseMove { dx: 0, dy: 0 },
    }
}

pub fn input_event_from_dto(dto: &InputEventDto) -> InputEvent {
    match dto {
        InputEventDto::MouseMove { dx, dy } => InputEvent::MouseMove { dx: *dx, dy: *dy },
        InputEventDto::MouseButton { button, pressed } => InputEvent::MouseButton {
            button: mouse_button_from_u8(*button),
            pressed: *pressed,
        },
        InputEventDto::MouseScroll { delta_x, delta_y } => InputEvent::MouseScroll {
            delta_x: *delta_x,
            delta_y: *delta_y,
        },
        InputEventDto::Key {
            code,
            pressed,
            ctrl,
            shift,
            alt,
            meta,
        } => InputEvent::Key {
            code: PortableKeyCode(*code),
            pressed: *pressed,
            modifiers: winx_domain::input_control::KeyModifiers {
                ctrl: *ctrl,
                shift: *shift,
                alt: *alt,
                meta: *meta,
            },
        },
        _ => InputEvent::MouseMove { dx: 0, dy: 0 },
    }
}

fn mouse_button_to_u8(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        MouseButton::X1 => 3,
        MouseButton::X2 => 4,
    }
}

fn mouse_button_from_u8(v: u8) -> MouseButton {
    match v {
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::X1,
        4 => MouseButton::X2,
        _ => MouseButton::Left,
    }
}

pub fn encode_input_payload(
    seq: u64,
    event: &InputEvent,
) -> Result<Vec<u8>, winx_protocol::ProtocolError> {
    let frame = winx_protocol::Frame::new(winx_protocol::Payload::Input(InputPayload {
        seq,
        event: input_event_to_dto(event),
    }));
    winx_protocol::encode(&frame)
}

pub fn encode_clipboard_payload(
    origin_peer_id: uuid::Uuid,
    content_hash: [u8; 32],
    text: &str,
) -> Result<Vec<u8>, winx_protocol::ProtocolError> {
    let frame = winx_protocol::Frame::new(winx_protocol::Payload::Clipboard(
        winx_protocol::ClipboardPayload {
            origin_peer_id,
            content_hash,
            text: text.to_string(),
        },
    ));
    winx_protocol::encode(&frame)
}
