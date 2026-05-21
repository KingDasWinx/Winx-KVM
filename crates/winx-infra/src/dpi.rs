//! Inicialização DPI antes da webview Tauri.

#![allow(unsafe_code)]

use tracing::warn;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

/// Garante coordenadas de monitor/cursor consistentes em setups multi-monitor HiDPI.
pub fn ensure_per_monitor_v2() {
    // SAFETY: constante documentada da API Win32.
    let ok = unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok()
    };
    if !ok {
        warn!("SetProcessDpiAwarenessContext falhou — coordenadas de monitor podem divergir");
    }
}
