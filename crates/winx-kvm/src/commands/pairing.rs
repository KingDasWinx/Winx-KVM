use serde::Serialize;
use tauri::State;
use winx_domain::shared::ids::{PeerId, SessionId};

use crate::app_state::PairingState;

#[derive(Debug, Serialize)]
pub struct PairingInitiatedDto {
    pub session_id: String,
    pub pin: String,
}

fn parse_peer_id(s: &str) -> Result<PeerId, String> {
    uuid::Uuid::parse_str(s)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("peer_id inválido: {e}"))
}

fn parse_session_id(s: &str) -> Result<SessionId, String> {
    uuid::Uuid::parse_str(s)
        .map(SessionId::from_uuid)
        .map_err(|e| format!("session_id inválido: {e}"))
}

fn parse_key_hex(hex: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex).map_err(|e| format!("hex inválido: {e}"))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| "chave deve ter 32 bytes (64 hex chars)".to_string())
}

fn map_err(e: winx_domain::DomainError) -> String {
    serde_json::to_string(&e).unwrap_or_else(|_| e.to_string())
}

#[tauri::command]
pub async fn initiate_pairing(
    state: State<'_, PairingState>,
    peer_id: String,
    peer_username: String,
) -> Result<PairingInitiatedDto, String> {
    let pid = parse_peer_id(&peer_id)?;
    let (session_id, pin) = state
        .pairing
        .initiate_pairing(pid, &peer_username)
        .await
        .map_err(map_err)?;
    Ok(PairingInitiatedDto {
        session_id: session_id.to_string(),
        pin,
    })
}

#[tauri::command]
pub async fn accept_pairing(
    state: State<'_, PairingState>,
    peer_id: String,
    peer_ephemeral_public_hex: String,
) -> Result<PairingInitiatedDto, String> {
    let pid = parse_peer_id(&peer_id)?;
    let peer_pub = parse_key_hex(&peer_ephemeral_public_hex)?;
    let (session_id, pin) = state
        .pairing
        .accept_pairing(pid, peer_pub)
        .await
        .map_err(map_err)?;
    Ok(PairingInitiatedDto {
        session_id: session_id.to_string(),
        pin,
    })
}

#[tauri::command]
pub async fn submit_pin(
    state: State<'_, PairingState>,
    session_id: String,
    pin: String,
    peer_public_key_hex: String,
    peer_username: String,
) -> Result<(), String> {
    let sid = parse_session_id(&session_id)?;
    let peer_key = parse_key_hex(&peer_public_key_hex)?;
    state
        .pairing
        .submit_pin(sid, &pin, peer_key, &peer_username)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn cancel_pairing(
    state: State<'_, PairingState>,
    session_id: String,
) -> Result<(), String> {
    let sid = parse_session_id(&session_id)?;
    state.pairing.cancel_pairing(sid).await.map_err(map_err)
}
