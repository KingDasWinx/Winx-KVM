//! Forwarder: consome o event bus interno e emite eventos para o frontend
//! via `app_handle.emit()`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::error;
use winx_domain::DomainEvent;

use crate::app_state::AppState;

/// Wrapper serializável para eventos enviados ao frontend.
///
/// O JS recebe `{ kind: string, ...payload }`. O `kind` é estável para
/// que o frontend possa fazer narrow type-safe.
#[derive(Debug, Clone, Serialize)]
struct FrontendEvent {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtt_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rx_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focus_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focus_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hotkey_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clipboard_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clipboard_byte_len: Option<usize>,
}

impl FrontendEvent {
    fn empty(kind: &'static str) -> Self {
        Self {
            kind,
            device_id: None,
            fingerprint: None,
            peer_id: None,
            peer_username: None,
            session_id: None,
            pin: None,
            rtt_ms: None,
            tx_bytes: None,
            rx_bytes: None,
            reason: None,
            focus_from: None,
            focus_to: None,
            lock_mode: None,
            hotkey_action: None,
            input_blocked: None,
            clipboard_hash: None,
            clipboard_byte_len: None,
        }
    }
}

fn content_hash_hex(hash: &winx_domain::data_exchange::ContentHash) -> String {
    hex::encode(hash.as_bytes())
}

fn focus_target_str(t: &winx_domain::input_control::FocusTarget) -> String {
    use winx_domain::input_control::FocusTarget;
    match t {
        FocusTarget::Local => "local".to_string(),
        FocusTarget::Remote(peer) => peer.to_string(),
    }
}

fn hotkey_action_str(a: winx_domain::input_control::HotkeyAction) -> String {
    use winx_domain::input_control::HotkeyAction;
    match a {
        HotkeyAction::PanicLocal => "panic_local".to_string(),
        HotkeyAction::ToggleLock => "toggle_lock".to_string(),
    }
}

#[allow(clippy::too_many_lines)]
impl From<&DomainEvent> for FrontendEvent {
    fn from(event: &DomainEvent) -> Self {
        match event {
            DomainEvent::DeviceCreated(e) => FrontendEvent {
                kind: "device-created",
                device_id: Some(e.device_id.to_string()),
                fingerprint: Some(e.fingerprint.clone()),
                ..FrontendEvent::empty("device-created")
            },
            DomainEvent::PeerForgotten(e) => FrontendEvent {
                kind: "peer-forgotten",
                device_id: Some(e.peer_id.to_string()),
                ..FrontendEvent::empty("peer-forgotten")
            },
            DomainEvent::PeerAppeared(e) => FrontendEvent {
                kind: "peers-updated",
                peer_id: Some(e.peer_id.to_string()),
                peer_username: Some(e.username.clone()),
                ..FrontendEvent::empty("peers-updated")
            },
            DomainEvent::PeerDisappeared(e) => FrontendEvent {
                kind: "peers-updated",
                peer_id: Some(e.peer_id.to_string()),
                ..FrontendEvent::empty("peers-updated")
            },
            DomainEvent::PeerUpdated(e) => FrontendEvent {
                kind: "peers-updated",
                peer_id: Some(e.peer_id.to_string()),
                peer_username: Some(e.username.clone()),
                ..FrontendEvent::empty("peers-updated")
            },
            DomainEvent::PairingRequested(e) => FrontendEvent {
                kind: "pairing-initiated",
                peer_id: Some(e.peer_id.to_string()),
                session_id: Some(e.session_id.to_string()),
                peer_username: Some(e.peer_username.clone()),
                pin: Some(e.pin.clone()),
                ..FrontendEvent::empty("pairing-initiated")
            },
            DomainEvent::PairingCompleted(e) => FrontendEvent {
                kind: "pairing-completed",
                peer_id: Some(e.peer_id.to_string()),
                peer_username: Some(e.peer_username.clone()),
                session_id: Some(e.session_id.to_string()),
                ..FrontendEvent::empty("pairing-completed")
            },
            DomainEvent::PairingCancelled(e) => FrontendEvent {
                kind: "pairing-cancelled",
                peer_id: Some(e.peer_id.to_string()),
                session_id: Some(e.session_id.to_string()),
                ..FrontendEvent::empty("pairing-cancelled")
            },
            DomainEvent::PairingFailed(e) => FrontendEvent {
                kind: "pairing-failed",
                peer_id: Some(e.peer_id.to_string()),
                session_id: Some(e.session_id.to_string()),
                ..FrontendEvent::empty("pairing-failed")
            },
            DomainEvent::ConnectionEstablished(e) => FrontendEvent {
                kind: "connection-established",
                peer_id: Some(e.peer_id.to_string()),
                peer_username: Some(e.peer_username.clone()),
                ..FrontendEvent::empty("connection-established")
            },
            DomainEvent::ConnectionLost(e) => FrontendEvent {
                kind: "connection-lost",
                peer_id: Some(e.peer_id.to_string()),
                reason: Some(e.reason.clone()),
                ..FrontendEvent::empty("connection-lost")
            },
            DomainEvent::StatsUpdated(e) => FrontendEvent {
                kind: "stats-updated",
                peer_id: Some(e.peer_id.to_string()),
                rtt_ms: Some(e.stats.rtt_ms),
                tx_bytes: Some(e.stats.tx_bytes),
                rx_bytes: Some(e.stats.rx_bytes),
                ..FrontendEvent::empty("stats-updated")
            },
            DomainEvent::FocusSwitched(e) => FrontendEvent {
                kind: "focus-switched",
                focus_from: Some(focus_target_str(&e.from)),
                focus_to: Some(focus_target_str(&e.to)),
                ..FrontendEvent::empty("focus-switched")
            },
            DomainEvent::HotkeyTriggered(e) => FrontendEvent {
                kind: "hotkey-triggered",
                hotkey_action: Some(hotkey_action_str(e.action)),
                ..FrontendEvent::empty("hotkey-triggered")
            },
            DomainEvent::InputBlocked(e) => FrontendEvent {
                kind: "input-blocked",
                peer_id: e.peer_id.map(|p| p.to_string()),
                input_blocked: Some(e.blocked),
                ..FrontendEvent::empty("input-blocked")
            },
            DomainEvent::ClipboardChanged(e) => FrontendEvent {
                kind: "clipboard-changed",
                clipboard_hash: Some(content_hash_hex(&e.hash)),
                clipboard_byte_len: Some(e.byte_len),
                ..FrontendEvent::empty("clipboard-changed")
            },
            DomainEvent::ClipboardReceived(e) => FrontendEvent {
                kind: "clipboard-received",
                peer_id: Some(e.from_peer.to_string()),
                clipboard_hash: Some(content_hash_hex(&e.hash)),
                ..FrontendEvent::empty("clipboard-received")
            },
            _ => FrontendEvent::empty("unknown"),
        }
    }
}

/// Inicia uma task que reencaminha cada `DomainEvent` do bus para o
/// frontend Tauri (event name: `winx://event`).
pub fn install_forwarder(handle: AppHandle) {
    let state = handle.state::<AppState>();
    let mut rx = state.bus.subscribe();

    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let payload = FrontendEvent::from(&event);
            if let Err(err) = handle.emit("winx://event", &payload) {
                error!(?err, "falha ao emitir evento para o frontend");
            }
        }
    });
}
