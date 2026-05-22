use serde::Serialize;
use tauri::State;
use winx_domain::workspace::{OwnershipMode, WorkspaceId};

use crate::app_state::WorkspaceState;

#[derive(Debug, Serialize)]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub owner_device_id: String,
    pub is_mirror: bool,
    pub is_orphan: bool,
    pub member_count: usize,
    pub version: u64,
}

#[derive(Debug, Serialize)]
pub struct PendingInviteDto {
    pub invite_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub sender_device_id: String,
    pub sender_username: String,
    pub sender_fingerprint_hex: String,
}

fn parse_workspace_id(s: &str) -> Result<WorkspaceId, String> {
    uuid::Uuid::parse_str(s)
        .map(WorkspaceId::from_uuid)
        .map_err(|e| format!("workspace_id inválido: {e}"))
}

fn parse_device_id(s: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s).map_err(|e| format!("device_id inválido: {e}"))
}

fn map_err(e: winx_domain::DomainError) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

fn ws_to_dto(ws: &winx_domain::workspace::Workspace) -> WorkspaceDto {
    let (is_mirror, is_orphan) = match &ws.ownership_mode {
        OwnershipMode::Original => (false, false),
        OwnershipMode::Mirror { is_orphan, .. } => (true, *is_orphan),
    };

    WorkspaceDto {
        id: ws.id.to_string(),
        name: ws.name.clone(),
        owner_device_id: ws.owner_device_id.to_string(),
        is_mirror,
        is_orphan,
        member_count: ws.members.len(),
        version: ws.version.as_u64(),
    }
}

#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<WorkspaceDto>, String> {
    let workspaces = state
        .service
        .list_workspaces()
        .await
        .map_err(|e| format!("falha ao listar workspaces: {e}"))?;

    Ok(workspaces.iter().map(ws_to_dto).collect())
}

#[tauri::command]
pub async fn create_workspace(
    state: State<'_, WorkspaceState>,
    name: String,
    peer_ids: Vec<String>,
) -> Result<WorkspaceDto, String> {
    let initial_members: Result<Vec<uuid::Uuid>, String> =
        peer_ids.iter().map(|pid| parse_device_id(pid)).collect();
    let initial_members = initial_members?;

    let ws = state
        .service
        .create_workspace(name, initial_members)
        .await
        .map_err(map_err)?;

    Ok(ws_to_dto(&ws))
}

#[tauri::command]
pub async fn invite_to_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    peer_id: String,
) -> Result<String, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let peer_device_id = parse_device_id(&peer_id)?;

    let invite_id = state
        .service
        .invite_to_workspace(ws_id, peer_device_id)
        .await
        .map_err(map_err)?;

    Ok(invite_id.to_string())
}

#[tauri::command]
pub async fn accept_invite(
    state: State<'_, WorkspaceState>,
    invite_id: String,
) -> Result<WorkspaceDto, String> {
    let invite_uuid =
        uuid::Uuid::parse_str(&invite_id).map_err(|e| format!("invite_id inválido: {e}"))?;

    let ws = state
        .service
        .accept_invite(invite_uuid)
        .await
        .map_err(map_err)?;

    Ok(ws_to_dto(&ws))
}

#[tauri::command]
pub async fn reject_invite(
    state: State<'_, WorkspaceState>,
    invite_id: String,
) -> Result<(), String> {
    let invite_uuid =
        uuid::Uuid::parse_str(&invite_id).map_err(|e| format!("invite_id inválido: {e}"))?;

    state
        .service
        .reject_invite(invite_uuid)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn connect_to_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    state
        .service
        .connect_to_workspace(ws_id)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn disconnect_from_workspace(state: State<'_, WorkspaceState>) -> Result<(), String> {
    state
        .service
        .disconnect_from_workspace()
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn force_disconnect_and_connect(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    state
        .service
        .force_disconnect_and_connect(ws_id)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    state.service.delete_workspace(ws_id).await.map_err(map_err)
}
