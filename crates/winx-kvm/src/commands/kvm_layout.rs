//! Commands de layout KVM (single connection) e enumeração de monitores locais.

use serde::Deserialize;
use tauri::State;
use winx_domain::shared::ids::PeerId;

use crate::app_state::{InputControlState, WorkspaceState};
use winx_domain::shared::ids::DeviceId;

use super::monitor_layout_dto::{
    dto_to_layout, dto_to_session, layout_to_dto, session_to_dto, MonitorLayoutDto,
    MonitorRectDto, SessionDesktopLayoutDto,
};

fn parse_peer_id(s: &str) -> Result<PeerId, String> {
    uuid::Uuid::parse_str(s)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("peer_id inválido: {e}"))
}

fn map_err(e: winx_domain::DomainError) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

#[tauri::command]
pub async fn list_local_monitors(
    state: State<'_, InputControlState>,
) -> Result<Vec<MonitorRectDto>, String> {
    let monitors = state
        .input_control
        .list_local_monitors()
        .await
        .map_err(map_err)?;
    Ok(monitors
        .iter()
        .map(super::monitor_layout_dto::rect_to_dto)
        .collect())
}

#[tauri::command]
pub async fn get_kvm_layout(
    state: State<'_, InputControlState>,
    peer_id: String,
) -> Result<Option<MonitorLayoutDto>, String> {
    let pid = parse_peer_id(&peer_id)?;
    let layout = state
        .input_control
        .get_kvm_layout(pid)
        .await
        .map_err(map_err)?;
    Ok(layout.as_ref().map(layout_to_dto))
}

#[tauri::command]
pub async fn get_peer_monitors(
    input_state: State<'_, InputControlState>,
    ws_state: State<'_, WorkspaceState>,
    peer_id: String,
    workspace_id: Option<String>,
) -> Result<Vec<MonitorRectDto>, String> {
    let peer_uuid = uuid::Uuid::parse_str(&peer_id)
        .map_err(|e| format!("peer_id inválido: {e}"))?;
    let device_id = DeviceId::from_uuid(peer_uuid);

    if let Some(ws_id_str) = workspace_id {
        let ws_id = uuid::Uuid::parse_str(&ws_id_str)
            .map_err(|e| format!("workspace_id inválido: {e}"))?;
        let ws_id = winx_domain::workspace::WorkspaceId::from_uuid(ws_id);
        let workspaces = ws_state
            .service
            .list_workspaces()
            .await
            .map_err(|e| format!("falha ao listar workspaces: {e}"))?;
        if let Some(ws) = workspaces.into_iter().find(|w| w.id == ws_id) {
            if let Some(layout) = ws.layout.get(device_id) {
                return Ok(layout
                    .local_monitors
                    .iter()
                    .map(super::monitor_layout_dto::rect_to_dto)
                    .collect());
            }
        }
    }

    let pid = parse_peer_id(&peer_id)?;
    let monitors = input_state
        .input_control
        .get_peer_monitors(pid)
        .await
        .map_err(map_err)?;
    Ok(monitors
        .iter()
        .map(super::monitor_layout_dto::rect_to_dto)
        .collect())
}

#[tauri::command]
pub async fn get_kvm_session_layout(
    state: State<'_, InputControlState>,
    peer_id: String,
) -> Result<Option<SessionDesktopLayoutDto>, String> {
    let pid = parse_peer_id(&peer_id)?;
    let layout = state
        .input_control
        .get_kvm_session_layout(pid)
        .await
        .map_err(map_err)?;
    Ok(layout.as_ref().map(session_to_dto))
}

#[derive(Debug, Deserialize)]
pub struct UpdateKvmSessionLayoutInput {
    pub peer_id: String,
    pub layout: SessionDesktopLayoutDto,
}

#[tauri::command]
pub async fn update_kvm_session_layout(
    state: State<'_, InputControlState>,
    input: UpdateKvmSessionLayoutInput,
) -> Result<(), String> {
    let pid = parse_peer_id(&input.peer_id)?;
    let session = dto_to_session(input.layout);

    state
        .input_control
        .ensure_layout_sync_for_peer(pid)
        .await
        .map_err(map_err)?;

    state
        .input_control
        .save_kvm_session_layout(pid, session)
        .await
        .map_err(map_err)?;

    if state.input_control.is_active_for_peer(pid).await {
        let _ = state.input_control.enable_for_peer(pid).await;
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct UpdateKvmLayoutInput {
    pub peer_id: String,
    pub layout: MonitorLayoutDto,
}

#[tauri::command]
pub async fn update_kvm_layout(
    state: State<'_, InputControlState>,
    input: UpdateKvmLayoutInput,
) -> Result<(), String> {
    let pid = parse_peer_id(&input.peer_id)?;
    let layout = dto_to_layout(input.layout)?;

    state
        .input_control
        .ensure_layout_sync_for_peer(pid)
        .await
        .map_err(map_err)?;

    state
        .input_control
        .save_kvm_layout(pid, layout)
        .await
        .map_err(map_err)?;

    if state.input_control.is_active_for_peer(pid).await {
        let _ = state.input_control.enable_for_peer(pid).await;
    }

    Ok(())
}
