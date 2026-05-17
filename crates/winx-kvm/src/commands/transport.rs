//! Commands do bounded context Transport.

use serde::Serialize;
use tauri::State;
use winx_domain::shared::ids::PeerId;

use crate::app_state::TransportState;

#[derive(Debug, Serialize)]
pub struct ConnectionStatsDto {
    pub rtt_ms: u32,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub lost_packets: u64,
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
pub async fn open_connection(
    state: State<'_, TransportState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(&peer_id)?;
    state.transport.connect_peer(pid).await.map_err(map_err)
}

#[tauri::command]
pub async fn disconnect_peer(
    state: State<'_, TransportState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(&peer_id)?;
    state.transport.disconnect_peer(pid).await.map_err(map_err)
}

#[tauri::command]
pub async fn get_connection_stats(
    state: State<'_, TransportState>,
    peer_id: String,
) -> Result<ConnectionStatsDto, String> {
    let pid = parse_peer_id(&peer_id)?;
    let connection_stats = state.transport.get_stats(pid).await.map_err(map_err)?;
    Ok(ConnectionStatsDto {
        rtt_ms: connection_stats.rtt_ms,
        tx_bytes: connection_stats.tx_bytes,
        rx_bytes: connection_stats.rx_bytes,
        lost_packets: connection_stats.lost_packets,
    })
}
