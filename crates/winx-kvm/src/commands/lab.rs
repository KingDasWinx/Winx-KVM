//! Commands da aba Lab (diagnóstico de conectividade).

use serde::Serialize;
use tauri::State;
use winx_application::{InputDebugStats, KeyboardMirrorStatus, LabProbeResults, ProbeResult};
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
    pub keys_hooked: u64,
    pub keys_send_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputDebugStatsDto {
    pub mirror_active: bool,
    pub keys_sent: u64,
    pub keys_hooked: u64,
    pub keys_send_errors: u64,
    pub remote_frames_received: u64,
    pub remote_inject_ok: u64,
    pub remote_inject_fail: u64,
    pub mouse_frames_sent: u64,
    pub input_enabled: bool,
    pub has_input_tx: bool,
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
            keys_hooked: s.keys_hooked,
            keys_send_errors: s.keys_send_errors,
        }
    }
}

impl From<InputDebugStats> for InputDebugStatsDto {
    fn from(s: InputDebugStats) -> Self {
        Self {
            mirror_active: s.mirror_active,
            keys_sent: s.keys_sent,
            keys_hooked: s.keys_hooked,
            keys_send_errors: s.keys_send_errors,
            remote_frames_received: s.remote_frames_received,
            remote_inject_ok: s.remote_inject_ok,
            remote_inject_fail: s.remote_inject_fail,
            mouse_frames_sent: s.mouse_frames_sent,
            input_enabled: s.input_enabled,
            has_input_tx: s.has_input_tx,
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
pub async fn get_input_debug_stats(
    input: State<'_, InputControlState>,
) -> Result<InputDebugStatsDto, String> {
    Ok(input.input_control.get_input_debug_stats().await.into())
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
