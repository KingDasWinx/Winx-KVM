//! Commands do contexto de discovery.

use serde::Serialize;
use tauri::State;

use crate::app_state::DiscoveryState;

/// DTO de um peer descoberto na rede para o frontend.
#[derive(Debug, Serialize)]
pub struct DiscoveredPeerDto {
    pub id: String,
    pub username: String,
    pub fingerprint: String,
    pub addresses: Vec<String>,
}

/// Retorna snapshot dos peers atualmente visíveis na rede.
#[tauri::command]
pub async fn list_discovered_peers(
    state: State<'_, DiscoveryState>,
) -> Result<Vec<DiscoveredPeerDto>, String> {
    let peers = state.discovery.get_peers().await;

    Ok(peers
        .into_iter()
        .map(|p| DiscoveredPeerDto {
            id: p.id.to_string(),
            username: p.username,
            fingerprint: p.fingerprint,
            addresses: p.addresses.iter().map(ToString::to_string).collect(),
        })
        .collect())
}
