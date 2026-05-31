//! Commands de layout KVM (single connection) e enumeração de monitores locais.

use serde::Deserialize;
use tauri::State;
use winx_domain::shared::ids::PeerId;

use crate::app_state::InputControlState;

use super::monitor_layout_dto::{dto_to_layout, layout_to_dto, MonitorLayoutDto, MonitorRectDto};

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
        .save_kvm_layout(pid, layout)
        .await
        .map_err(map_err)
}
