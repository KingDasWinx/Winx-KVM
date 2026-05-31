use crate::shared::ids::DeviceId;
use crate::workspace::WorkspaceId;
use serde::{Deserialize, Serialize};

/// Tipos de mudanças em um workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceChangeKind {
    Renamed,
    LayoutUpdated,
    MembersChanged,
}

/// Workspace foi criado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceCreated {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub owner_device_id: DeviceId,
    pub ownership_mode_kind: String, // "original" ou "mirror"
}

/// Workspace foi atualizado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceUpdated {
    pub workspace_id: WorkspaceId,
    pub version: u64,
    pub change_kind: WorkspaceChangeKind,
}

/// Workspace foi deletado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDeleted {
    pub workspace_id: WorkspaceId,
}

/// Membro se juntou ao workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberJoined {
    pub workspace_id: WorkspaceId,
    pub device_id: DeviceId,
    pub username: String,
}

/// Membro saiu do workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberLeft {
    pub workspace_id: WorkspaceId,
    pub device_id: DeviceId,
}

/// Invite de workspace foi enviado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteSent {
    pub invite_id: crate::workspace::InviteId,
    pub workspace_id: WorkspaceId,
    pub target_device_id: DeviceId,
}

/// Invite foi recebido (do lado do receptor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteIncoming {
    pub invite_id: crate::workspace::InviteId,
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    pub sender_device_id: DeviceId,
    pub sender_username: String,
    pub sender_fingerprint_hex: String,
}

/// Invite foi aceito.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteAccepted {
    pub invite_id: crate::workspace::InviteId,
    pub workspace_id: WorkspaceId,
    pub accepting_device_id: DeviceId,
}

/// Invite foi rejeitado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteRejected {
    pub invite_id: crate::workspace::InviteId,
}

/// Invite expirou.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteExpired {
    pub invite_id: crate::workspace::InviteId,
}

/// Mouse global se moveu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalCursorMoved {
    pub workspace_id: WorkspaceId,
    pub x: i32,
    pub y: i32,
    pub seq: u64,
}

/// Workspace foi marcado como órfão.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMarkedOrphan {
    pub workspace_id: WorkspaceId,
}

/// Sync foi aplicado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSyncApplied {
    pub workspace_id: WorkspaceId,
    pub new_version: u64,
    pub workspace_name: String,
    /// `true` quando aplicado via `handle_workspace_sync` (remoto); `false` em edit local.
    pub from_remote: bool,
}

/// Sync foi descartado (versão local >= versão remota).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSyncDiscarded {
    pub workspace_id: WorkspaceId,
    pub local_version: u64,
    pub incoming_version: u64,
}

/// Device se conectou a um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConnected {
    pub workspace_id: WorkspaceId,
}

/// Device se desconectou de um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDisconnected {
    pub workspace_id: WorkspaceId,
}

/// Tentativa de conexão com conflito (já conectado em outro workspace).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConnectionConflict {
    pub active_id: WorkspaceId,
    pub target_id: WorkspaceId,
}

/// Estado de presença de um membro do workspace mudou (online/offline).
///
/// Emitido pelo `presence_watcher` quando `owner_last_seen` cruza o threshold
/// de 30s sem heartbeat/sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberPresenceChanged {
    pub workspace_id: WorkspaceId,
    pub device_id: DeviceId,
    pub is_online: bool,
}
