use serde::{Deserialize, Serialize};
use tauri::State;
use winx_domain::input_control::layout::MonitorLayout;
use winx_domain::shared::ids::DeviceId;
use winx_domain::shared::DomainErrorCode;
use winx_domain::workspace::{OwnershipMode, WorkspaceId, WorkspaceLayout};

use crate::app_state::{InputControlState, TransportState, WorkspaceState};

#[derive(Debug, Serialize)]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub owner_device_id: String,
    pub is_mirror: bool,
    pub is_orphan: bool,
    pub owner_username: Option<String>,
    pub member_count: usize,
    pub version: u64,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberInput {
    pub workspace_id: String,
    pub device_id: String,
    pub public_key_hex: String,
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceMemberDto {
    pub device_id: String,
    pub public_key_hex: String,
    pub username: String,
    pub is_owner: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceLayoutDto {
    pub per_device: std::collections::BTreeMap<String, MonitorLayoutDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorLayoutDto {
    pub local_monitors: Vec<MonitorRectDto>,
    pub remote_peer: String,
    pub remote_virtual: MonitorRectDto,
    pub edge: EdgeConfigDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRectDto {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfigDto {
    pub local_exit: String,
    pub remote_entry: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceLayoutInput {
    pub workspace_id: String,
    pub device_id: String,
    pub layout: MonitorLayoutDto,
}

fn layout_to_dto(layout: &MonitorLayout) -> MonitorLayoutDto {
    use winx_domain::input_control::layout::BorderSide;

    fn border_str(side: BorderSide) -> String {
        match side {
            BorderSide::Right => "Right".to_string(),
            BorderSide::Left => "Left".to_string(),
            BorderSide::Top => "Top".to_string(),
            BorderSide::Bottom => "Bottom".to_string(),
        }
    }

    fn rect_to_dto(r: &winx_domain::input_control::MonitorRect) -> MonitorRectDto {
        MonitorRectDto {
            id: r.id.0,
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    MonitorLayoutDto {
        local_monitors: layout.local_monitors.iter().map(rect_to_dto).collect(),
        remote_peer: layout.remote_peer.to_string(),
        remote_virtual: rect_to_dto(&layout.remote_virtual),
        edge: EdgeConfigDto {
            local_exit: border_str(layout.edge.local_exit),
            remote_entry: border_str(layout.edge.remote_entry),
        },
    }
}

fn dto_to_layout(dto: MonitorLayoutDto) -> Result<MonitorLayout, String> {
    use winx_domain::input_control::layout::{BorderSide, EdgeConfig};
    use winx_domain::input_control::{MonitorId, MonitorRect};
    use winx_domain::shared::ids::PeerId;

    fn parse_border(s: &str) -> Result<BorderSide, String> {
        match s {
            "Right" => Ok(BorderSide::Right),
            "Left" => Ok(BorderSide::Left),
            "Top" => Ok(BorderSide::Top),
            "Bottom" => Ok(BorderSide::Bottom),
            other => Err(format!("border side inválido: {other}")),
        }
    }

    fn rect_from_dto(r: MonitorRectDto) -> MonitorRect {
        MonitorRect {
            id: MonitorId(r.id),
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    let remote_peer = uuid::Uuid::parse_str(&dto.remote_peer)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("remote_peer inválido: {e}"))?;

    Ok(MonitorLayout {
        local_monitors: dto.local_monitors.into_iter().map(rect_from_dto).collect(),
        remote_peer,
        remote_virtual: rect_from_dto(dto.remote_virtual),
        edge: EdgeConfig {
            local_exit: parse_border(&dto.edge.local_exit)?,
            remote_entry: parse_border(&dto.edge.remote_entry)?,
        },
    })
}

fn workspace_layout_to_dto(layout: &WorkspaceLayout) -> WorkspaceLayoutDto {
    WorkspaceLayoutDto {
        per_device: layout
            .per_device
            .iter()
            .map(|(device_id, monitor_layout)| {
                (device_id.to_string(), layout_to_dto(monitor_layout))
            })
            .collect(),
    }
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

/// Conecta QUIC ao peer remoto do workspace e habilita input control com layout salvo.
async fn activate_workspace_kvm(
    ws_state: &WorkspaceState,
    transport_state: &TransportState,
    input_state: &InputControlState,
    ws_id: WorkspaceId,
) -> Result<(), String> {
    let ws = ws_state.service.get_workspace(ws_id).await.map_err(map_err)?;
    let primary = ws_state.service.primary_remote_peer(&ws).ok_or_else(|| {
        map_err(winx_domain::DomainError::new(
            DomainErrorCode::InternalError,
            "workspace has no remote member for KVM",
        ))
    })?;

    if !transport_state
        .transport
        .is_peer_connected(primary)
        .await
    {
        transport_state
            .transport
            .connect_peer(primary)
            .await
            .map_err(map_err)?;
    }

    input_state
        .input_control
        .enable_for_peer(primary)
        .await
        .map_err(map_err)?;

    Ok(())
}

fn ws_to_dto(ws: &winx_domain::workspace::Workspace) -> WorkspaceDto {
    let (is_mirror, is_orphan, owner_username) = match &ws.ownership_mode {
        OwnershipMode::Original => (false, false, None),
        OwnershipMode::Mirror {
            owner_username_snapshot,
            is_orphan,
            ..
        } => (true, *is_orphan, Some(owner_username_snapshot.clone())),
    };

    WorkspaceDto {
        id: ws.id.to_string(),
        name: ws.name.clone(),
        owner_device_id: ws.owner_device_id.to_string(),
        is_mirror,
        is_orphan,
        owner_username,
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
pub async fn rename_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    new_name: String,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::Rename { new_name },
        )
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
    ws_state: State<'_, WorkspaceState>,
    transport_state: State<'_, TransportState>,
    input_state: State<'_, InputControlState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    ws_state
        .service
        .connect_to_workspace(ws_id)
        .await
        .map_err(map_err)?;

    activate_workspace_kvm(&ws_state, &transport_state, &input_state, ws_id).await
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
    ws_state: State<'_, WorkspaceState>,
    transport_state: State<'_, TransportState>,
    input_state: State<'_, InputControlState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    ws_state
        .service
        .force_disconnect_and_connect(ws_id)
        .await
        .map_err(map_err)?;

    activate_workspace_kvm(&ws_state, &transport_state, &input_state, ws_id).await
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;

    state.service.delete_workspace(ws_id).await.map_err(map_err)
}

#[tauri::command]
pub async fn add_workspace_member(
    state: State<'_, WorkspaceState>,
    input: AddMemberInput,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&input.workspace_id)?;
    let device_uuid = parse_device_id(&input.device_id)?;
    let pubkey_bytes =
        hex::decode(&input.public_key_hex).map_err(|e| format!("public_key_hex inválido: {e}"))?;
    let pubkey_arr: [u8; 32] = pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "public_key_hex deve ter 32 bytes".to_string())?;

    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::AddMember {
                device_id: winx_domain::shared::ids::DeviceId::from_uuid(device_uuid),
                public_key: winx_domain::identity::key::PublicKey::new(pubkey_arr),
                username: input.username,
            },
        )
        .await
        .map_err(map_err)?;
    Ok(ws_to_dto(&ws))
}

#[tauri::command]
pub async fn remove_workspace_member(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    device_id: String,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let device_uuid = parse_device_id(&device_id)?;
    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::RemoveMember {
                device_id: winx_domain::shared::ids::DeviceId::from_uuid(device_uuid),
            },
        )
        .await
        .map_err(map_err)?;
    Ok(ws_to_dto(&ws))
}

#[tauri::command]
pub async fn forget_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    state.service.forget_workspace(ws_id).await.map_err(map_err)
}

#[tauri::command]
pub async fn list_workspace_members(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<Vec<WorkspaceMemberDto>, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let workspaces = state
        .service
        .list_workspaces()
        .await
        .map_err(|e| format!("falha ao listar workspaces: {e}"))?;
    let ws = workspaces
        .iter()
        .find(|w| w.id == ws_id)
        .ok_or_else(|| "workspace não encontrado".to_string())?;
    Ok(ws
        .members
        .iter()
        .map(|m| WorkspaceMemberDto {
            device_id: m.device_id.to_string(),
            public_key_hex: hex::encode(m.public_key.as_bytes()),
            username: m.username_cache.clone(),
            is_owner: m.device_id == ws.owner_device_id,
        })
        .collect())
}

#[tauri::command]
pub async fn get_workspace_layout(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<WorkspaceLayoutDto, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let ws = state
        .service
        .list_workspaces()
        .await
        .map_err(|e| format!("falha ao listar workspaces: {e}"))?
        .into_iter()
        .find(|w| w.id == ws_id)
        .ok_or_else(|| "workspace não encontrado".to_string())?;

    Ok(workspace_layout_to_dto(&ws.layout))
}

#[tauri::command]
pub async fn update_workspace_layout(
    ws_state: State<'_, WorkspaceState>,
    input_state: State<'_, InputControlState>,
    input: UpdateWorkspaceLayoutInput,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&input.workspace_id)?;
    let device_uuid = parse_device_id(&input.device_id)?;
    let layout = dto_to_layout(input.layout)?;

    let ws = ws_state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::UpdateLayout {
                device_id: DeviceId::from_uuid(device_uuid),
                layout,
            },
        )
        .await
        .map_err(map_err)?;

    if ws_state.service.active_workspace_id().await == Some(ws_id) {
        if let Some(peer) = ws_state.service.primary_remote_peer(&ws) {
            let _ = input_state.input_control.enable_for_peer(peer).await;
        }
    }

    Ok(ws_to_dto(&ws))
}
