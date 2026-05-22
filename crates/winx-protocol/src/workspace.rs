use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const WORKSPACE_INVITE_PROTOCOL_VERSION: u16 = 1;
pub const WORKSPACE_INVITE_MAGIC: [u8; 4] = *b"WIWX"; // Winx Invite WX

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkspaceInviteMessage {
    Invite(WorkspaceInvitePayload),
    Response(WorkspaceInviteResponsePayload),
    Cancel(WorkspaceInviteCancelPayload),
}

/// Invite sent to a peer to join a workspace.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInvitePayload {
    pub invite_id: Uuid,
    pub workspace_snapshot: WorkspaceSnapshotPayload,
    pub sender_device_id: Uuid,
    pub sender_username: String,
    pub sender_pubkey: [u8; 32],
    pub target_device_id: Uuid,
}

/// Workspace state snapshot for replication to new mirror.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceSnapshotPayload {
    pub id: Uuid,
    pub name: String,
    pub owner_device_id: Uuid,
    pub owner_username: String,
    pub version: u64,
    pub members: Vec<MemberSnapshotPayload>,
}

/// Member snapshot in workspace.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberSnapshotPayload {
    pub device_id: Uuid,
    pub public_key: [u8; 32],
    pub username: String,
    pub joined_at_rfc3339: String,
}

/// Response to workspace invite.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInviteResponsePayload {
    pub invite_id: Uuid,
    pub responder_device_id: Uuid,
    pub responder_pubkey: [u8; 32],
    pub accepted: bool,
}

/// Cancel a pending invite.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInviteCancelPayload {
    pub invite_id: Uuid,
}
