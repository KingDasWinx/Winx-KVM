//! Captura via low-level hooks e injeção via `SendInput`.

#![allow(
    unsafe_code,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::cast_lossless,
    clippy::default_trait_access,
    clippy::unnecessary_cast
)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, error, info, warn};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, GetLastError, HINSTANCE, POINT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_EXTENDEDKEY,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, VK_HOME, VK_SCROLL, VK_END,
    VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU,
    MapVirtualKeyW, MAPVK_VK_TO_VSC_EX, MOUSEEVENTF_ABSOLUTE,
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RAWMOUSE,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RID_INPUT, RIM_TYPEMOUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, ClipCursor, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetSystemMetrics, PeekMessageW, RegisterClassW, SetCursorPos, SetForegroundWindow,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, UnregisterClassW, WindowFromPoint,
    CS_HREDRAW, CS_VREDRAW, HHOOK,
    KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_REMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_INPUT,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_MOUSEMOVE, WM_QUIT, WM_HOTKEY,
    LLMHF_INJECTED, LLKHF_INJECTED, SM_CXSCREEN, SM_CYSCREEN, WINDOW_EX_STYLE, WINDOW_STYLE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WNDCLASSW,
};
use winx_application::ports::{CaptureHandle, InputBackend};
use winx_domain::input_control::{
    HotkeyAction, InputEvent, KeyModifiers, MouseButton, MOUSE_SEND_MIN_MANHATTAN,
};

use crate::cursor_trap::{hide_cursor_trap, release_cursor_trap_sync, show_cursor_trap};
use crate::input_vk_map::{portable_to_vk, vk_to_portable};

/// Assinatura única para eventos injetados internamente pelo KVM
pub const KVM_SIGNATURE: usize = 0xDEAD_C0DE;

/// `KBDLLHOOKSTRUCT.flags` — evento gerado por `SendInput` / injeção remota.
#[must_use]
pub const fn kb_hook_flags_is_injected(flags: u32) -> bool {
    (flags & LLKHF_INJECTED.0 as u32) != 0
}


static PASS_THROUGH: AtomicBool = AtomicBool::new(true);
static HOOK_TX: OnceLock<Sender<HookMsg>> = OnceLock::new();
static LAST_MOUSE_X: AtomicI32 = AtomicI32::new(0);
static LAST_MOUSE_Y: AtomicI32 = AtomicI32::new(0);
static HAVE_LAST_MOUSE: AtomicBool = AtomicBool::new(false);
static SKIP_MOUSE_DELTA: AtomicBool = AtomicBool::new(false);
static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
static ALT_DOWN: AtomicBool = AtomicBool::new(false);
static CURSOR_HIDDEN: AtomicBool = AtomicBool::new(false);
/// Foco remoto no sender: deltas vêm de Raw Input, não de `pt` no clip.
static USE_RAW_MOUSE_DELTA: AtomicBool = AtomicBool::new(false);
static RAW_MOUSE_REGISTERED: AtomicBool = AtomicBool::new(false);
static RAW_INPUT_HWND: AtomicIsize = AtomicIsize::new(0);
static RAW_INPUT_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
const RAW_INPUT_CLASS_NAME: &str = "WinxKvmRawInputHost";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hwnd_to_isize(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn isize_to_hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

fn module_instance() -> windows::core::Result<HINSTANCE> {
    // SAFETY: módulo do processo atual.
    unsafe { GetModuleHandleW(None).map(|m| HINSTANCE(m.0)) }
}

/// Janela message-only na thread do hook — destino de `WM_INPUT` com `RIDEV_INPUTSINK`.
fn ensure_raw_input_host_window() -> anyhow::Result<HWND> {
    let existing = RAW_INPUT_HWND.load(Ordering::SeqCst);
    if existing != 0 {
        return Ok(isize_to_hwnd(existing));
    }

    if !RAW_INPUT_CLASS_REGISTERED.load(Ordering::SeqCst) {
        let name = wide(RAW_INPUT_CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(raw_input_host_wnd_proc),
            hInstance: module_instance().unwrap_or_default(),
            lpszClassName: PCWSTR(name.as_ptr()),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        // SAFETY: classe única no processo.
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            let err = unsafe { GetLastError() };
            anyhow::bail!("RegisterClassW RawInputHost falhou: {err:?}");
        }
        RAW_INPUT_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    }

    let class = wide(RAW_INPUT_CLASS_NAME);
    let ex_style = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
    let style = WINDOW_STYLE(WS_POPUP.0);

    // SAFETY: classe registrada nesta thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(ex_style.0),
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            style,
            0,
            0,
            1,
            1,
            None,
            None,
            module_instance()?,
            None,
        )?
    };

    RAW_INPUT_HWND.store(hwnd_to_isize(hwnd), Ordering::SeqCst);
    info!(?hwnd, "janela Raw Input host criada na thread do hook");
    Ok(hwnd)
}

unsafe extern "system" fn raw_input_host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT && USE_RAW_MOUSE_DELTA.load(Ordering::SeqCst) {
        if let Some(tx) = HOOK_TX.get() {
            dispatch_raw_mouse_input(tx, lparam);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn destroy_raw_input_host_window() {
    let raw = RAW_INPUT_HWND.swap(0, Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    let hwnd = isize_to_hwnd(raw);
    // SAFETY: HWND criado por este módulo na thread do hook.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    if RAW_INPUT_CLASS_REGISTERED.swap(false, Ordering::SeqCst) {
        let name = wide(RAW_INPUT_CLASS_NAME);
        // SAFETY: classe registrada neste processo.
        if unsafe { UnregisterClassW(PCWSTR(name.as_ptr()), module_instance().unwrap_or_default()) }
            .is_err()
        {
            warn!("UnregisterClassW RawInputHost falhou");
        }
    }
}

/// Restauração incondicional e síncrona dos recursos de hardware.
/// Garante que o usuário recupere o controle físico mesmo se a tarefa Tokio travar.
pub fn unconditional_release_hardware_sync() {
    PASS_THROUGH.store(true, Ordering::SeqCst);
    release_cursor_trap_sync();
    unsafe {
        let _ = ClipCursor(None);
        // Restaurar cursor se estava oculto
        if CURSOR_HIDDEN.load(Ordering::SeqCst) {
            let mut c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
            while c < 0 {
                c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
            }
            CURSOR_HIDDEN.store(false, Ordering::SeqCst);
        }
    }
    info!("liberação incondicional de hardware executada");
}

/// Delta de mouse com aritmética que não panic em hooks (debug overflow).
pub(crate) fn mouse_delta(lx: i32, ly: i32, pt_x: i32, pt_y: i32) -> (i32, i32) {
    (pt_x.wrapping_sub(lx), pt_y.wrapping_sub(ly))
}

/// Próximo `WM_MOUSEMOVE` após clip/warp não gera delta espúrio.
pub fn reset_mouse_delta_baseline() {
    SKIP_MOUSE_DELTA.store(true, Ordering::SeqCst);
}

#[derive(Debug)]
enum HookMsg {
    Input(InputEvent),
    HotkeyPanic,
    HotkeyLock,
    HotkeyForceReset,
    Stop,
}

pub struct Win32InputBackend {
    pass_through: Arc<AtomicBool>,
    hook_tx: Sender<HookMsg>,
    hook_rx: Receiver<HookMsg>,
    thread: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    next_handle: AtomicU64,
}

impl Win32InputBackend {
    pub fn new() -> Self {
        let (hook_tx, hook_rx) = crossbeam_channel::bounded(4096);
        let _ = HOOK_TX.set(hook_tx.clone());
        Self {
            pass_through: Arc::new(AtomicBool::new(true)),
            hook_tx,
            hook_rx,
            thread: Arc::new(std::sync::Mutex::new(None)),
            next_handle: AtomicU64::new(1),
        }
    }
}

impl Default for Win32InputBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Win32InputBackend {
    fn drop(&mut self) {
        let _ = self.hook_tx.send(HookMsg::Stop);
        release_cursor_trap_sync();
        unsafe {
            let _ = ClipCursor(None);
            if CURSOR_HIDDEN.load(Ordering::SeqCst) {
                let mut c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
                while c < 0 {
                    c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
                }
                CURSOR_HIDDEN.store(false, Ordering::SeqCst);
            }
        }
    }
}

#[async_trait]
impl InputBackend for Win32InputBackend {
    async fn start_capture(
        &self,
        on_event: Box<dyn Fn(InputEvent) + Send + Sync>,
        on_hotkey: Box<dyn Fn(HotkeyAction) + Send + Sync>,
    ) -> anyhow::Result<CaptureHandle> {
        let id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let rx = self.hook_rx.clone();
        let pass = Arc::clone(&self.pass_through);
        PASS_THROUGH.store(pass.load(Ordering::SeqCst), Ordering::SeqCst);

        if self.thread.lock().unwrap().is_none() {
            let hook_tx = self.hook_tx.clone();
            let handle = thread::spawn(move || hook_thread_main(hook_tx));
            *self.thread.lock().unwrap() = Some(handle);
        }

        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match msg {
                    HookMsg::Input(ev) => on_event(ev),
                    HookMsg::HotkeyPanic => on_hotkey(HotkeyAction::PanicLocal),
                    HookMsg::HotkeyLock => on_hotkey(HotkeyAction::ToggleLock),
                    HookMsg::HotkeyForceReset => on_hotkey(HotkeyAction::force_reset()),
                    HookMsg::Stop => break,
                }
            }
            error!("thread de repasse de hooks encerrou — reinicie o app");
        });

        Ok(CaptureHandle { id })
    }

    async fn stop_capture(&self, _handle: CaptureHandle) -> anyhow::Result<()> {
        let _ = self.hook_tx.send(HookMsg::Stop);
        Ok(())
    }

    async fn inject(&self, event: InputEvent) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || inject_event(event))
            .await
            .map_err(|e| anyhow::anyhow!("join: {e}"))?
    }

    async fn set_cursor_clipped(&self, rect: Option<(i32, i32, u32, u32)>) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || {
            if let Some((x, y, w, h)) = rect {
                if let Err(e) = show_cursor_trap() {
                    warn!(?e, "cursor trap falhou — usando apenas ClipCursor");
                }
                let clip = windows::Win32::Foundation::RECT {
                    left: x,
                    top: y,
                    right: x + w as i32,
                    bottom: y + h as i32,
                };
                // SAFETY: RECT válido.
                unsafe {
                    ClipCursor(Some(&clip))?;
                }
            } else {
                hide_cursor_trap();
                // SAFETY: liberar cursor.
                unsafe {
                    ClipCursor(None)?;
                }
            }
            reset_mouse_delta_baseline();
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))?
    }

    async fn transition_to_remote(
        &self,
        center_x: i32,
        center_y: i32,
        clip_rect: (i32, i32, u32, u32),
    ) -> anyhow::Result<()> {
        if let Err(e) = self.hide_cursor_system().await {
            warn!(?e, "falha ao ocultar cursor (não-crítico, continuando)");
        }
        self.warp_cursor_signed(center_x, center_y).await?;
        self.set_cursor_clipped(Some(clip_rect)).await?;
        self.reset_mouse_delta_baseline();
        Ok(())
    }

    fn set_raw_mouse_capture(&self, enabled: bool) {
        if enabled {
            let hwnd = isize_to_hwnd(RAW_INPUT_HWND.load(Ordering::SeqCst));
            if hwnd.0.is_null() {
                warn!("Raw Input host HWND ainda não criado — fallback hook pt (USE_RAW_MOUSE_DELTA não alterado)");
                return;
            }
            register_raw_mouse_device(hwnd);
            // Só ativa o flag após registro bem-sucedido para evitar janela onde
            // USE_RAW_MOUSE_DELTA=true mas WM_INPUT ainda não chega.
            USE_RAW_MOUSE_DELTA.store(true, Ordering::SeqCst);
        } else {
            USE_RAW_MOUSE_DELTA.store(false, Ordering::SeqCst);
            unregister_raw_mouse_device();
        }
    }

    async fn transition_to_local(
        &self,
        edge_x: i32,
        primary_center_x: i32,
        primary_center_y: i32,
    ) -> anyhow::Result<()> {
        self.set_cursor_clipped(None).await?;
        self.restore_cursor_system().await?;
        let safe_x = if edge_x > primary_center_x {
            edge_x - 10
        } else {
            edge_x + 10
        };
        self.warp_cursor_signed(safe_x, primary_center_y).await?;
        self.reset_mouse_delta_baseline();
        Ok(())
    }

    async fn warp_cursor(&self, x: i32, y: i32) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || {
            // SAFETY: coordenadas de tela válidas.
            unsafe {
                SetCursorPos(x, y)?;
            }
            reset_mouse_delta_baseline();
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))?
    }

    fn reset_mouse_delta_baseline(&self) {
        reset_mouse_delta_baseline();
    }

    fn set_pass_through(&self, pass_through: bool) {
        self.pass_through.store(pass_through, Ordering::SeqCst);
        PASS_THROUGH.store(pass_through, Ordering::SeqCst);
        // info!(swallow = !pass_through, "swallow mode");
    }

    async fn set_cursor_visible(&self, visible: bool) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || unsafe {
            let already_hidden = CURSOR_HIDDEN.load(Ordering::SeqCst);
            if !visible && !already_hidden {
                let mut c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(false);
                while c >= 0 {
                    c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(false);
                }
                CURSOR_HIDDEN.store(true, Ordering::SeqCst);
                info!("cursor ocultado");
            } else if visible && already_hidden {
                let mut c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
                while c < 0 {
                    c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
                }
                CURSOR_HIDDEN.store(false, Ordering::SeqCst);
                info!("cursor restaurado");
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))?
    }

    async fn hide_cursor_system(&self) -> anyhow::Result<()> {
        self.set_cursor_visible(false).await
    }

    async fn restore_cursor_system(&self) -> anyhow::Result<()> {
        self.set_cursor_visible(true).await
    }

    async fn warp_cursor_signed(&self, x: i32, y: i32) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || {
            unsafe {
                let screen_width = GetSystemMetrics(SM_CXSCREEN);
                let screen_height = GetSystemMetrics(SM_CYSCREEN);

                if screen_width == 0 || screen_height == 0 {
                    anyhow::bail!("Dimensões de monitor inválidas");
                }

                let norm_x = ((x as i64) * 65535 / (screen_width as i64)) as i32;
                let norm_y = ((y as i64) * 65535 / (screen_height as i64)) as i32;

                let input_packet = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: norm_x,
                            dy: norm_y,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                            time: 0,
                            dwExtraInfo: KVM_SIGNATURE,
                        },
                    },
                };

                let executed = SendInput(&[input_packet], std::mem::size_of::<INPUT>() as i32);
                if executed != 1 {
                    anyhow::bail!("SendInput assinado falhou: {}", executed);
                }
                info!(x, y, norm_x, norm_y, "teleporte de cursor assinado executado");
                Ok(())
            }
        })
        .await?
    }
}

fn send_inputs(inputs: &[INPUT]) -> anyhow::Result<()> {
    // SAFETY: slice INPUT válido.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        let err = unsafe { GetLastError() };
        anyhow::bail!("SendInput enviou {sent}/{} last_err={:#x}", inputs.len(), err.0);
    }
    Ok(())
}

fn inject_event(event: InputEvent) -> anyhow::Result<()> {
    match event {
        InputEvent::MouseMove { dx, dy, .. } => {
            let mv = InputEvent::MouseMove {
                dx,
                dy,
                screen_x: 0,
                screen_y: 0,
            };
            if mv.is_noop_mouse_move() || mv.is_noise_mouse_move(MOUSE_SEND_MIN_MANHATTAN) {
                return Ok(());
            }
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        dwFlags: MOUSEEVENTF_MOVE,
                        dwExtraInfo: KVM_SIGNATURE,
                        ..Default::default()
                    },
                },
            };
            // SAFETY: INPUT válido.
            send_inputs(&[input])?;
        }
        InputEvent::MouseButton { button, pressed } => {
            let flags = match (button, pressed) {
                (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                _ => return Ok(()),
            };
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: flags,
                        dwExtraInfo: KVM_SIGNATURE,
                        ..Default::default()
                    },
                },
            };
            send_inputs(&[input])?;
        }
        InputEvent::MouseScroll {
            delta_x: _,
            delta_y,
        } => {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        mouseData: delta_y as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        dwExtraInfo: KVM_SIGNATURE,
                        ..Default::default()
                    },
                },
            };
            send_inputs(&[input])?;
        }
        InputEvent::Key {
            code,
            pressed,
            modifiers: _,
        } => {
            use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, KEYBD_EVENT_FLAGS};
            let vk = portable_to_vk(code);
            let scan_full = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
            let mut flags = if scan_full != 0 { KEYEVENTF_SCANCODE } else { KEYBD_EVENT_FLAGS(0) };
            let extended = scan_full & 0xE000 != 0;
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !pressed {
                flags |= KEYEVENTF_KEYUP;
            }
            let scan_byte = (scan_full & 0xFF) as u16;

            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: if scan_full == 0 { VIRTUAL_KEY(vk as u16) } else { VIRTUAL_KEY(0) },
                        wScan: scan_byte,
                        dwFlags: flags,
                        dwExtraInfo: KVM_SIGNATURE,
                        ..Default::default()
                    },
                },
            };
            if scan_full == 0 {
                warn!(target: "winx::input::remote", ?vk, "vk sem scancode mapeado — fallback para wVk");
            }
            send_inputs(&[input])?;
            debug!(
                target: "winx::input::remote",
                ?vk,
                pressed,
                "inject tecla ok"
            );
        }
        InputEvent::MouseWarpAbsolute { x, y } => {
            // SAFETY: coordenadas de tela do receiver; normalização idêntica ao warp_cursor_signed.
            unsafe {
                let screen_width = GetSystemMetrics(SM_CXSCREEN);
                let screen_height = GetSystemMetrics(SM_CYSCREEN);
                if screen_width > 0 && screen_height > 0 {
                    let norm_x = ((x as i64) * 65535 / (screen_width as i64)) as i32;
                    let norm_y = ((y as i64) * 65535 / (screen_height as i64)) as i32;
                    let input_packet = INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                dx: norm_x,
                                dy: norm_y,
                                mouseData: 0,
                                dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                                time: 0,
                                dwExtraInfo: KVM_SIGNATURE,
                            },
                        },
                    };
                    send_inputs(&[input_packet])?;

                    // Ativar a janela sob o cursor para que teclas subsequentes sejam entregues.
                    let hwnd = WindowFromPoint(POINT { x, y });
                    if !hwnd.is_invalid() {
                        let _ = SetForegroundWindow(hwnd);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn register_raw_mouse_device(hwnd: HWND) -> bool {
    if RAW_MOUSE_REGISTERED.swap(true, Ordering::SeqCst) {
        return true;
    }
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    let cb_size = std::mem::size_of::<RAWINPUTDEVICE>() as u32;
    // SAFETY: array válido; `cbSize` é tamanho da estrutura (API Win32).
    let result = unsafe { RegisterRawInputDevices(&[device], cb_size) };
    if let Err(err) = result {
        let last = unsafe { GetLastError() };
        warn!(
            ?err,
            ?last,
            ?hwnd,
            cb_size,
            dw_flags = RIDEV_INPUTSINK.0,
            "RegisterRawInputDevices falhou — fallback para hook pt"
        );
        RAW_MOUSE_REGISTERED.store(false, Ordering::SeqCst);
        false
    } else {
        info!(?hwnd, "Raw Input mouse registrado (INPUTSINK)");
        true
    }
}

fn unregister_raw_mouse_device() {
    if !RAW_MOUSE_REGISTERED.swap(false, Ordering::SeqCst) {
        return;
    }
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: HWND::default(),
    };
    let cb_size = std::mem::size_of::<RAWINPUTDEVICE>() as u32;
    // SAFETY: MSDN — `RIDEV_REMOVE` com usage page/usage correspondentes.
    if let Err(err) = unsafe { RegisterRawInputDevices(&[device], cb_size) } {
        warn!(?err, "unregister RegisterRawInputDevices falhou");
    }
}

/// Processa `WM_INPUT` e envia deltas relativos do hardware (foco remoto no sender).
fn dispatch_raw_mouse_input(hook_tx: &Sender<HookMsg>, lparam: LPARAM) {
    let handle = windows::Win32::UI::Input::HRAWINPUT(lparam.0 as *mut _);
    let mut size = 0u32;
    // SAFETY: size query.
    let status = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            None,
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if status == u32::MAX || size == 0 {
        return;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buffer sized by GetRawInputData.
    let read = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some(buf.as_mut_ptr().cast()),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || read == 0 {
        return;
    }
    // SAFETY: `read` bytes inicializados pelo SO.
    let raw = unsafe { &*(buf.as_ptr().cast::<RAWINPUT>()) };
    if raw.header.dwType != u32::from(RIM_TYPEMOUSE.0) {
        return;
    }
    // SAFETY: tipo mouse confirmado.
    let mouse: RAWMOUSE = unsafe { raw.data.mouse };
    // MOUSE_MOVE_RELATIVE == 0x0000; testar bit 0 ausente indica movimento relativo.
    // Ignorar eventos absolutos (tablets, touchscreen, injeções absolutas).
    const MOUSE_MOVE_ABSOLUTE_FLAG: u16 = 0x0001;
    if mouse.usFlags.0 & MOUSE_MOVE_ABSOLUTE_FLAG != 0 {
        return;
    }
    let dx = mouse.lLastX;
    let dy = mouse.lLastY;
    let ev = InputEvent::MouseMove {
        dx,
        dy,
        screen_x: 0,
        screen_y: 0,
    };
    if ev.is_noop_mouse_move() || ev.is_noise_mouse_move(MOUSE_SEND_MIN_MANHATTAN) {
        return;
    }
    let _ = hook_tx.try_send(HookMsg::Input(ev));
}

fn hook_thread_main(hook_tx: Sender<HookMsg>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_hook_loop(&hook_tx)
    }));

    release_cursor_trap_sync();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::ClipCursor(None);
        if CURSOR_HIDDEN.load(Ordering::SeqCst) {
            let mut c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
            while c < 0 {
                c = windows::Win32::UI::WindowsAndMessaging::ShowCursor(true);
            }
            CURSOR_HIDDEN.store(false, Ordering::SeqCst);
        }
    }

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => error!(?err, "thread de hooks encerrou com erro"),
        Err(panic) => error!(?panic, "hook thread panicou — cursor restaurado"),
    }
}

fn run_hook_loop(_hook_tx: &Sender<HookMsg>) -> anyhow::Result<()> {
    let raw_hwnd = ensure_raw_input_host_window()?;
    RAW_INPUT_HWND.store(hwnd_to_isize(raw_hwnd), Ordering::SeqCst);
    // Não pré-registra Raw Input aqui: set_raw_mouse_capture(true) registra quando
    // o foco vai para remoto, evitando que RAW_MOUSE_REGISTERED já seja true e
    // impeça o RegisterRawInputDevices no momento certo.

    // SAFETY: módulo válido.
    unsafe {
        let instance = GetModuleHandleW(None)?;

        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0)?;
        let kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0)?;

        info!("hooks Win32 instalados");

        // Registrar hotkeys globais como fallback para casos onde o WH_KEYBOARD_LL é desabilitado
        let hotkey_panic_ok =
            RegisterHotKey(None, 1, MOD_CONTROL | MOD_ALT, u32::from(VK_HOME.0)).is_ok();
        let hotkey_reset_ok =
            RegisterHotKey(None, 2, MOD_CONTROL | MOD_ALT, u32::from(VK_END.0)).is_ok();
        let hotkey_scroll_ok =
            RegisterHotKey(None, 3, MOD_NOREPEAT, u32::from(VK_SCROLL.0)).is_ok();
        if !hotkey_panic_ok {
            warn!("RegisterHotKey Ctrl+Alt+Home falhou — apenas hook inline ativo");
        }
        if !hotkey_reset_ok {
            warn!("RegisterHotKey Ctrl+Alt+End falhou — apenas hook inline ativo");
        }
        if !hotkey_scroll_ok {
            warn!("RegisterHotKey Scroll Lock falhou — apenas hook inline ativo");
        }

        let mut msg = MSG::default();
        'message_loop: loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'message_loop;
                }
                if msg.message == WM_INPUT && USE_RAW_MOUSE_DELTA.load(Ordering::SeqCst) {
                    if let Some(tx) = HOOK_TX.get() {
                        dispatch_raw_mouse_input(tx, msg.lParam);
                    }
                    continue;
                }
                if msg.message == WM_HOTKEY {
                    if let Some(tx) = HOOK_TX.get() {
                        match msg.wParam.0 as u32 {
                            1 => {
                                let _ = tx.try_send(HookMsg::HotkeyPanic);
                                info!("hotkey via RegisterHotKey disparou: Ctrl+Alt+Home");
                            }
                            2 => {
                                let _ = tx.try_send(HookMsg::HotkeyForceReset);
                                info!("hotkey via RegisterHotKey disparou: Ctrl+Alt+End");
                            }
                            3 => {
                                let _ = tx.try_send(HookMsg::HotkeyLock);
                                info!("hotkey via RegisterHotKey disparou: Scroll Lock");
                            }
                            _ => {}
                        }
                    }
                } else {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                break;
            }
            if msg.message == WM_INPUT && USE_RAW_MOUSE_DELTA.load(Ordering::SeqCst) {
                if let Some(tx) = HOOK_TX.get() {
                    dispatch_raw_mouse_input(tx, msg.lParam);
                }
                continue;
            }
            if msg.message == WM_HOTKEY {
                if let Some(tx) = HOOK_TX.get() {
                    match msg.wParam.0 as u32 {
                        1 => {
                            let _ = tx.try_send(HookMsg::HotkeyPanic);
                            info!("hotkey via RegisterHotKey disparou: Ctrl+Alt+Home");
                        }
                        2 => {
                            let _ = tx.try_send(HookMsg::HotkeyForceReset);
                            info!("hotkey via RegisterHotKey disparou: Ctrl+Alt+End");
                        }
                        3 => {
                            let _ = tx.try_send(HookMsg::HotkeyLock);
                            info!("hotkey via RegisterHotKey disparou: Scroll Lock");
                        }
                        _ => {}
                    }
                }
            } else {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        if hotkey_panic_ok {
            let _ = UnregisterHotKey(None, 1);
        }
        if hotkey_reset_ok {
            let _ = UnregisterHotKey(None, 2);
        }
        if hotkey_scroll_ok {
            let _ = UnregisterHotKey(None, 3);
        }
        UnhookWindowsHookEx(mouse_hook)?;
        UnhookWindowsHookEx(kb_hook)?;
        let _ = ClipCursor(None);
    }

    unregister_raw_mouse_device();
    destroy_raw_input_host_window();
    Ok(())
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };

        // Camada de Filtragem 1: Ignora inputs emulados padrão pelo SO
        let is_injected = (info.flags & LLMHF_INJECTED) != 0;

        // Camada de Filtragem 2: Ignora inputs assinados internamente pelo KVM
        let is_own_input = info.dwExtraInfo == KVM_SIGNATURE;

        if is_injected || is_own_input {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }

        let swallow = !PASS_THROUGH.load(Ordering::SeqCst);
        if let Some(tx) = HOOK_TX.get() {
            let ev = match wparam.0 as u32 {
                x if x == WM_MOUSEMOVE as u32 => {
                    let (dx, dy) = if SKIP_MOUSE_DELTA.swap(false, Ordering::SeqCst) {
                        (0, 0)
                    } else if HAVE_LAST_MOUSE.load(Ordering::SeqCst) {
                        let lx = LAST_MOUSE_X.load(Ordering::SeqCst);
                        let ly = LAST_MOUSE_Y.load(Ordering::SeqCst);
                        mouse_delta(lx, ly, info.pt.x, info.pt.y)
                    } else {
                        HAVE_LAST_MOUSE.store(true, Ordering::SeqCst);
                        (0, 0)
                    };
                    LAST_MOUSE_X.store(info.pt.x, Ordering::SeqCst);
                    LAST_MOUSE_Y.store(info.pt.y, Ordering::SeqCst);
                    if USE_RAW_MOUSE_DELTA.load(Ordering::SeqCst) {
                        None
                    } else {
                        let ev = InputEvent::MouseMove {
                            dx,
                            dy,
                            screen_x: info.pt.x,
                            screen_y: info.pt.y,
                        };
                        if ev.is_noop_mouse_move()
                            || ev.is_noise_mouse_move(MOUSE_SEND_MIN_MANHATTAN)
                        {
                            None
                        } else {
                            Some(ev)
                        }
                    }
                }
                0x0201 => Some(InputEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed: true,
                }),
                0x0202 => Some(InputEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed: false,
                }),
                0x0204 => Some(InputEvent::MouseButton {
                    button: MouseButton::Right,
                    pressed: true,
                }),
                0x0205 => Some(InputEvent::MouseButton {
                    button: MouseButton::Right,
                    pressed: false,
                }),
                0x020A => Some(InputEvent::MouseScroll {
                    delta_x: 0,
                    delta_y: ((info.mouseData >> 16) as i16) as i32,
                }),
                _ => None,
            };
            if let Some(ev) = ev {
                let _ = tx.try_send(HookMsg::Input(ev));
            }
        }
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let vk = info.vkCode as u32;
        let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
        let pressed = msg != WM_KEYUP as u32 && msg != WM_SYSKEYUP as u32;

        if kb_hook_flags_is_injected(info.flags.0 as u32) {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }
        // Rastrear modificadores
        match vk {
            v if v == VK_LCONTROL.0 as u32 || v == VK_RCONTROL.0 as u32 || v == VK_CONTROL.0 as u32 => {
                CTRL_DOWN.store(down, Ordering::SeqCst);
            }
            v if v == VK_LMENU.0 as u32 || v == VK_RMENU.0 as u32 || v == VK_MENU.0 as u32 => {
                ALT_DOWN.store(down, Ordering::SeqCst);
            }
            _ => {}
        }

        // Hotkeys inline (independem de PASS_THROUGH)
        if down && CTRL_DOWN.load(Ordering::SeqCst) && ALT_DOWN.load(Ordering::SeqCst) {
            if vk == VK_HOME.0 as u32 {
                if let Some(tx) = HOOK_TX.get() {
                    let _ = tx.try_send(HookMsg::HotkeyPanic);
                    info!("hotkey inline disparado: Ctrl+Alt+Home");
                }
                return LRESULT(1);
            }
            if vk == VK_END.0 as u32 {
                if let Some(tx) = HOOK_TX.get() {
                    let _ = tx.try_send(HookMsg::HotkeyForceReset);
                    info!("hotkey inline disparado: Ctrl+Alt+End");
                }
                return LRESULT(1);
            }
        }
        if down && vk == VK_SCROLL.0 as u32 {
            if let Some(tx) = HOOK_TX.get() {
                let _ = tx.try_send(HookMsg::HotkeyLock);
                info!("hotkey inline disparado: Scroll Lock");
            }
            return LRESULT(1);
        }

        let swallow = !PASS_THROUGH.load(Ordering::SeqCst);
        if let Some(tx) = HOOK_TX.get() {
            if let Some(portable_code) = vk_to_portable(info.vkCode as u16) {
                let ev = InputEvent::Key {
                    code: portable_code,
                    pressed,
                    modifiers: KeyModifiers::default(),
                };
                let _ = tx.try_send(HookMsg::Input(ev));
            }
        }
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

#[cfg(test)]
mod delta_tests {
    use super::{kb_hook_flags_is_injected, mouse_delta};
    use windows::Win32::UI::WindowsAndMessaging::LLKHF_INJECTED;

    #[test]
    fn kb_injected_flag_detected() {
        assert!(kb_hook_flags_is_injected(LLKHF_INJECTED.0 as u32));
        assert!(!kb_hook_flags_is_injected(0));
    }

    #[test]
    fn subtract_overflow_case_does_not_panic() {
        let _ = mouse_delta(2_000_000_000, 0, -500_000_000, 0);
    }

    #[test]
    fn clip_teleport_backward_delta() {
        let (dx, dy) = mouse_delta(2568, 100, 100, 100);
        assert_eq!(dx, -2468);
        assert_eq!(dy, 0);
    }
}
