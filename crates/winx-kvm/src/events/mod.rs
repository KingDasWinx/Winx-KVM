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
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invite_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    other_workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    y: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_from_remote: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_inbound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    monitor_count: Option<usize>,
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
            workspace_id: None,
            workspace_name: None,
            invite_id: None,
            other_workspace_id: None,
            new_version: None,
            is_online: None,
            x: None,
            y: None,
            seq: None,
            sync_from_remote: None,
            is_inbound: None,
            monitor_count: None,
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
        HotkeyAction::ForceReset => "force_reset".to_string(),
        HotkeyAction::OpenActiveWorkspace => "open_active_workspace".to_string(),
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
            DomainEvent::PairingIncoming(e) => FrontendEvent {
                kind: "pairing-incoming",
                peer_id: Some(e.peer_id.to_string()),
                session_id: Some(e.session_id.to_string()),
                peer_username: Some(e.peer_username.clone()),
                ..FrontendEvent::empty("pairing-incoming")
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
                workspace_id: e.via_workspace_id.map(|id| id.to_string()),
                is_inbound: Some(e.is_inbound),
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
            DomainEvent::PeerMonitorsUpdated(e) => FrontendEvent {
                kind: "peer-monitors-updated",
                peer_id: Some(e.peer_id.to_string()),
                monitor_count: Some(e.monitor_count),
                ..FrontendEvent::empty("peer-monitors-updated")
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
            // Workspace events
            DomainEvent::WorkspaceInviteIncoming(e) => FrontendEvent {
                kind: "workspace-invite-incoming",
                invite_id: Some(e.invite_id.to_string()),
                workspace_id: Some(e.workspace_id.to_string()),
                workspace_name: Some(e.workspace_name.clone()),
                peer_id: Some(e.sender_device_id.to_string()),
                peer_username: Some(e.sender_username.clone()),
                fingerprint: Some(e.sender_fingerprint_hex.clone()),
                ..FrontendEvent::empty("workspace-invite-incoming")
            },
            DomainEvent::WorkspaceConnected(e) => FrontendEvent {
                kind: "workspace-connected",
                workspace_id: Some(e.workspace_id.to_string()),
                ..FrontendEvent::empty("workspace-connected")
            },
            DomainEvent::WorkspaceDisconnected(e) => FrontendEvent {
                kind: "workspace-disconnected",
                workspace_id: Some(e.workspace_id.to_string()),
                ..FrontendEvent::empty("workspace-disconnected")
            },
            DomainEvent::WorkspaceConnectionConflict(e) => FrontendEvent {
                kind: "workspace-connection-conflict",
                workspace_id: Some(e.target_id.to_string()),
                other_workspace_id: Some(e.active_id.to_string()),
                ..FrontendEvent::empty("workspace-connection-conflict")
            },
            DomainEvent::WorkspaceCreated(_) | DomainEvent::WorkspaceDeleted(_) => FrontendEvent {
                kind: "workspaces-updated",
                ..FrontendEvent::empty("workspaces-updated")
            },
            DomainEvent::WorkspaceSyncApplied(e) => FrontendEvent {
                kind: "workspaces-updated",
                workspace_id: Some(e.workspace_id.to_string()),
                workspace_name: Some(e.workspace_name.clone()),
                new_version: Some(e.new_version),
                sync_from_remote: Some(e.from_remote),
                ..FrontendEvent::empty("workspaces-updated")
            },
            DomainEvent::WorkspaceSyncDiscarded(e) => FrontendEvent {
                kind: "workspaces-updated",
                workspace_id: Some(e.workspace_id.to_string()),
                ..FrontendEvent::empty("workspaces-updated")
            },
            DomainEvent::WorkspaceMarkedOrphan(e) => FrontendEvent {
                kind: "workspace-marked-orphan",
                workspace_id: Some(e.workspace_id.to_string()),
                ..FrontendEvent::empty("workspace-marked-orphan")
            },
            DomainEvent::WorkspaceMemberPresenceChanged(e) => FrontendEvent {
                kind: "workspace-member-presence",
                workspace_id: Some(e.workspace_id.to_string()),
                peer_id: Some(e.device_id.to_string()),
                is_online: Some(e.is_online),
                ..FrontendEvent::empty("workspace-member-presence")
            },
            DomainEvent::WorkspaceInviteAccepted(e) => FrontendEvent {
                kind: "workspace-invite-accepted",
                invite_id: Some(e.invite_id.to_string()),
                ..FrontendEvent::empty("workspace-invite-accepted")
            },
            DomainEvent::WorkspaceInviteRejected(e) => FrontendEvent {
                kind: "workspace-invite-rejected",
                invite_id: Some(e.invite_id.to_string()),
                ..FrontendEvent::empty("workspace-invite-rejected")
            },
            DomainEvent::WorkspaceInviteExpired(e) => FrontendEvent {
                kind: "workspace-invite-expired",
                invite_id: Some(e.invite_id.to_string()),
                ..FrontendEvent::empty("workspace-invite-expired")
            },
            DomainEvent::WorkspaceGlobalCursorMoved(e) => FrontendEvent {
                kind: "workspace-global-cursor",
                workspace_id: Some(e.workspace_id.to_string()),
                x: Some(e.x),
                y: Some(e.y),
                seq: Some(e.seq),
                ..FrontendEvent::empty("workspace-global-cursor")
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
