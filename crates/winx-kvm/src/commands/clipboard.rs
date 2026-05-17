//! Commands do bounded context DataExchange (clipboard).

use std::sync::Arc;

use tauri::State;
use winx_domain::shared::ids::PeerId;
use winx_infra::TomlConfigStore;

use crate::app_state::ClipboardState;

fn parse_peer_id(s: &str) -> Result<PeerId, String> {
    uuid::Uuid::parse_str(s)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("peer_id inválido: {e}"))
}

fn map_err(e: winx_domain::DomainError) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

#[tauri::command]
pub async fn get_clipboard_auto_sync(state: State<'_, ClipboardState>) -> Result<bool, String> {
    Ok(state.clipboard.get_auto_sync().await)
}

#[tauri::command]
pub async fn set_clipboard_auto_sync(
    state: State<'_, ClipboardState>,
    config_store: State<'_, Arc<TomlConfigStore>>,
    enabled: bool,
) -> Result<(), String> {
    state.clipboard.set_auto_sync(enabled).await;
    config_store
        .save_clipboard_auto_sync(enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn enable_clipboard_sync(
    state: State<'_, ClipboardState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(&peer_id)?;
    state.clipboard.enable_for_peer(pid).await.map_err(map_err)
}
