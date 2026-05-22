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
}

/// Sync foi descartado (versão local >= versão remota).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSyncDiscarded {
    pub workspace_id: WorkspaceId,
    pub local_version: u64,
    pub incoming_version: u64,
}
