//! Commands da aba Lab (diagnóstico de conectividade).

use serde::Serialize;
use tauri::State;
use winx_application::{KeyboardMirrorStatus, LabProbeResults, ProbeResult};
use winx_domain::shared::ids::PeerId;

use crate::app_state::{InputControlState, LabState};

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResultDto {
    pub service: String,
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabProbeResultsDto {
    pub peer_id: String,
    pub probes: Vec<ProbeResultDto>,
    pub ran_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardMirrorStatusDto {
    pub active: bool,
    pub seconds_left: u32,
    pub keys_sent: u64,
}

impl From<ProbeResult> for ProbeResultDto {
    fn from(p: ProbeResult) -> Self {
        Self {
            service: p.service,
            ok: p.ok,
            latency_ms: p.latency_ms,
            detail: p.detail,
        }
    }
}

impl From<LabProbeResults> for LabProbeResultsDto {
    fn from(r: LabProbeResults) -> Self {
        Self {
            peer_id: r.peer_id,
            probes: r.probes.into_iter().map(Into::into).collect(),
            ran_at_ms: r.ran_at_ms,
        }
    }
}

impl From<KeyboardMirrorStatus> for KeyboardMirrorStatusDto {
    fn from(s: KeyboardMirrorStatus) -> Self {
        Self {
            active: s.active,
            seconds_left: s.seconds_left,
            keys_sent: s.keys_sent,
        }
    }
}

fn parse_peer_id(peer_id: String) -> Result<PeerId, String> {
    let uuid = uuid::Uuid::parse_str(&peer_id).map_err(|e| format!("peer_id inválido: {e}"))?;
    Ok(PeerId::from_uuid(uuid))
}

#[tauri::command]
pub async fn run_connectivity_suite(
    lab: State<'_, LabState>,
    peer_id: String,
) -> Result<LabProbeResultsDto, String> {
    let pid = parse_peer_id(peer_id)?;
    Ok(lab.lab.run_suite(pid).await.into())
}

#[tauri::command]
pub async fn start_keyboard_mirror_test(
    input: State<'_, InputControlState>,
    peer_id: String,
    duration_secs: Option<u32>,
) -> Result<(), String> {
    let pid = parse_peer_id(peer_id)?;
    let secs = duration_secs.unwrap_or(5);
    input
        .input_control
        .start_keyboard_mirror(pid, secs)
        .await
        .map_err(|e| serde_json::to_string(&e).unwrap_or_else(|_| e.message))
}

#[tauri::command]
pub async fn get_keyboard_mirror_status(
    input: State<'_, InputControlState>,
) -> Result<KeyboardMirrorStatusDto, String> {
    Ok(input
        .input_control
        .get_keyboard_mirror_status()
        .await
        .into())
}

#[tauri::command]
pub async fn send_test_click(
    input: State<'_, InputControlState>,
    peer_id: String,
) -> Result<(), String> {
    let pid = parse_peer_id(peer_id)?;
    input
        .input_control
        .send_test_click(pid)
        .await
        .map_err(|e| serde_json::to_string(&e).unwrap_or_else(|_| e.message))
}
