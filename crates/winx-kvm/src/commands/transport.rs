//! Commands do bounded context Transport.

use serde::Serialize;
use tauri::State;
use winx_domain::{shared::ids::PeerId, transport::ConnectionState};

use crate::app_state::{InputControlState, TransportState};

#[derive(Debug, Serialize)]
pub struct ConnectionStateDto {
    pub peer_id: String,
    /// `connecting` | `connected` | `reconnecting` | `disconnected`
    pub status: String,
    pub rtt_ms: Option<u32>,
    pub tx_bytes: Option<u64>,
    pub rx_bytes: Option<u64>,
}

fn connection_state_to_status(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Connected { .. } => "connected",
        ConnectionState::Reconnecting { .. } => "reconnecting",
        ConnectionState::Disconnected => "disconnected",
    }
}

#[tauri::command]
pub async fn list_connection_states(
    state: State<'_, TransportState>,
) -> Result<Vec<ConnectionStateDto>, String> {
    let list = state.transport.list_connection_snapshots().await;
    Ok(list
        .into_iter()
        .map(|(peer_id, conn_state, stats)| {
            let include_stats = matches!(conn_state, ConnectionState::Connected { .. });
            ConnectionStateDto {
                peer_id: peer_id.to_string(),
                status: connection_state_to_status(&conn_state).to_string(),
                rtt_ms: include_stats.then_some(stats.rtt_ms),
                tx_bytes: include_stats.then_some(stats.tx_bytes),
                rx_bytes: include_stats.then_some(stats.rx_bytes),
            }
        })
        .collect())
}

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
    transport: State<'_, TransportState>,
    input: State<'_, InputControlState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(&peer_id)?;
    input.input_control.reset_after_disconnect(pid).await;
    transport
        .transport
        .disconnect_peer(pid)
        .await
        .map_err(map_err)
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
