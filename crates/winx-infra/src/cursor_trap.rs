//! Janela topmost transparente cobrindo a área virtual da tela (fallback F6.9).

#![allow(
    unsafe_code,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::items_after_statements
)]

use std::sync::atomic::{AtomicIsize, Ordering};

use tracing::{info, warn};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, ShowWindow, UnregisterClassW,
    CS_HREDRAW, CS_VREDRAW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WNDCLASSW,
    GetSystemMetrics,
};

static TRAP_HWND: AtomicIsize = AtomicIsize::new(0);
static TRAP_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

const TRAP_CLASS_NAME: &str = "WinxKvmCursorTrap";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn module_instance() -> windows::core::Result<HINSTANCE> {
    // SAFETY: módulo do processo atual.
    unsafe { GetModuleHandleW(None).map(|m| HINSTANCE(m.0)) }
}

fn hwnd_to_isize(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn isize_to_hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

unsafe extern "system" fn trap_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            TRAP_HWND.store(0, Ordering::SeqCst);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn ensure_class_registered() -> bool {
    if TRAP_CLASS_REGISTERED.load(Ordering::SeqCst) {
        return true;
    }
    let name = wide(TRAP_CLASS_NAME);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(trap_wnd_proc),
        hInstance: module_instance().unwrap_or_default(),
        lpszClassName: PCWSTR(name.as_ptr()),
        style: CS_HREDRAW | CS_VREDRAW,
        ..Default::default()
    };
    // SAFETY: WNDCLASSW válido; nome único do processo.
    let atom = unsafe { RegisterClassW(&wc) };
    if atom != 0 {
        TRAP_CLASS_REGISTERED.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

fn virtual_screen_rect() -> (i32, i32, i32, i32) {
    // SAFETY: métricas do sistema sempre disponíveis em sessão interativa.
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (x, y, w, h)
    }
}

/// Exibe janela trap full-screen (topmost, transparente, sem foco).
pub fn show_cursor_trap() -> anyhow::Result<()> {
    if TRAP_HWND.load(Ordering::SeqCst) != 0 {
        return Ok(());
    }
    if !ensure_class_registered() {
        anyhow::bail!("RegisterClassW falhou para cursor trap");
    }

    let (x, y, w, h) = virtual_screen_rect();
    let class = wide(TRAP_CLASS_NAME);
    let ex_style = WS_EX_LAYERED
        | WS_EX_TRANSPARENT
        | WS_EX_TOPMOST
        | WS_EX_NOACTIVATE
        | WS_EX_TOOLWINDOW;
    let style = WINDOW_STYLE(WS_POPUP.0);

    // SAFETY: classe registrada; coordenadas de tela válidas.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(ex_style.0),
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            module_instance()?,
            None,
        )?
    };

    // SAFETY: HWND válido retornado por CreateWindowExW.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    TRAP_HWND.store(hwnd_to_isize(hwnd), Ordering::SeqCst);
    info!(x, y, w, h, "cursor trap window exibida");
    Ok(())
}

/// Oculta e destrói a janela trap.
pub fn hide_cursor_trap() {
    let raw = TRAP_HWND.swap(0, Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    let hwnd = isize_to_hwnd(raw);
    // SAFETY: HWND obtido deste módulo; DestroyWindow após swap.
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = DestroyWindow(hwnd);
    }
    info!("cursor trap window ocultada");
}

/// Liberação síncrona no shutdown/panic.
pub fn release_cursor_trap_sync() {
    hide_cursor_trap();
    if TRAP_CLASS_REGISTERED.load(Ordering::SeqCst) {
        let name = wide(TRAP_CLASS_NAME);
        // SAFETY: nome da classe registrada neste processo.
        if unsafe {
            UnregisterClassW(PCWSTR(name.as_ptr()), module_instance().unwrap_or_default())
        }
        .is_err()
        {
            warn!("UnregisterClassW cursor trap falhou");
        }
        TRAP_CLASS_REGISTERED.store(false, Ordering::SeqCst);
    }
}
