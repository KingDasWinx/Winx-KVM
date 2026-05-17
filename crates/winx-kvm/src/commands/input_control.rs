//! Commands do bounded context InputControl.

use serde::Serialize;
use tauri::State;
use winx_domain::{
    input_control::{FocusState, FocusTarget},
    shared::ids::PeerId,
};

use crate::app_state::InputControlState;

#[derive(Debug, Serialize)]
pub struct FocusStateDto {
    /// `"local"` ou UUID do peer remoto.
    pub target: String,
    pub lock_mode: bool,
}

fn focus_target_to_str(target: &FocusTarget) -> String {
    match target {
        FocusTarget::Local => "local".to_string(),
        FocusTarget::Remote(peer) => peer.to_string(),
    }
}

impl From<FocusState> for FocusStateDto {
    fn from(state: FocusState) -> Self {
        Self {
            target: focus_target_to_str(&state.target),
            lock_mode: state.lock_mode,
        }
    }
}

fn parse_peer_id(s: &str) -> Result<PeerId, String> {
    uuid::Uuid::parse_str(s)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("peer_id inválido: {e}"))
}

fn map_err(e: winx_domain::DomainError) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

#[tauri::command]
pub async fn get_focus_state(state: State<'_, InputControlState>) -> Result<FocusStateDto, String> {
    Ok(state.input_control.get_focus_state().await.into())
}

#[tauri::command]
pub async fn enable_input_control(
    state: State<'_, InputControlState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(&peer_id)?;
    state
        .input_control
        .enable_for_peer(pid)
        .await
        .map_err(map_err)
}
