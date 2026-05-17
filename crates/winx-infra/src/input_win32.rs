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

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use tracing::{error, info};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, VK_HOME, VK_SCROLL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, ClipCursor, DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_REMOVE,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_HOTKEY, WM_KEYUP, WM_MOUSEMOVE, WM_QUIT,
};
use winx_application::ports::{CaptureHandle, InputBackend};
use winx_domain::input_control::{HotkeyAction, InputEvent, KeyModifiers, MouseButton};

use crate::input_vk_map::{portable_to_vk, vk_to_portable};

const HOTKEY_PANIC: i32 = 1;
const HOTKEY_LOCK: i32 = 2;
static PASS_THROUGH: AtomicBool = AtomicBool::new(true);
static HOOK_TX: OnceLock<Sender<HookMsg>> = OnceLock::new();
static LAST_MOUSE_X: AtomicI32 = AtomicI32::new(0);
static LAST_MOUSE_Y: AtomicI32 = AtomicI32::new(0);
static HAVE_LAST_MOUSE: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
enum HookMsg {
    Input(InputEvent),
    HotkeyPanic,
    HotkeyLock,
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
        let (hook_tx, hook_rx) = crossbeam_channel::unbounded();
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
                    HookMsg::Stop => break,
                }
            }
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
                // SAFETY: liberar cursor.
                unsafe {
                    ClipCursor(None)?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))?
    }

    fn set_pass_through(&self, pass_through: bool) {
        self.pass_through.store(pass_through, Ordering::SeqCst);
        PASS_THROUGH.store(pass_through, Ordering::SeqCst);
    }
}

fn send_inputs(inputs: &[INPUT]) -> anyhow::Result<()> {
    // SAFETY: slice INPUT válido.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        anyhow::bail!("SendInput enviou {sent} de {}", inputs.len());
    }
    Ok(())
}

fn inject_event(event: InputEvent) -> anyhow::Result<()> {
    match event {
        InputEvent::MouseMove { dx, dy, .. } => {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        dwFlags: MOUSEEVENTF_MOVE,
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
            let vk = portable_to_vk(code);
            let mut flags = KEYEVENTF_SCANCODE;
            if !pressed {
                flags |= KEYEVENTF_KEYUP;
            }
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: vk,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            };
            send_inputs(&[input])?;
        }
        _ => {}
    }
    Ok(())
}

fn hook_thread_main(hook_tx: Sender<HookMsg>) {
    if let Err(err) = run_hook_loop(&hook_tx) {
        error!(?err, "thread de hooks encerrou com erro");
    }
}

fn run_hook_loop(hook_tx: &Sender<HookMsg>) -> anyhow::Result<()> {
    // SAFETY: módulo válido.
    unsafe {
        let instance = GetModuleHandleW(None)?;

        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0)?;
        let kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0)?;

        use windows::Win32::UI::Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL,
        };
        RegisterHotKey(
            HWND::default(),
            HOTKEY_PANIC,
            MOD_CONTROL | MOD_ALT,
            VK_HOME.0 as u32,
        )?;
        RegisterHotKey(
            HWND::default(),
            HOTKEY_LOCK,
            Default::default(),
            VK_SCROLL.0 as u32,
        )?;

        info!("hooks Win32 instalados");

        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_HOTKEY {
                    let id = msg.wParam.0 as i32;
                    match id {
                        HOTKEY_PANIC => {
                            let _ = hook_tx.send(HookMsg::HotkeyPanic);
                        }
                        HOTKEY_LOCK => {
                            let _ = hook_tx.send(HookMsg::HotkeyLock);
                        }
                        _ => {}
                    }
                }
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if GetMessageW(&mut msg, None, 0, 0).0 == 0 {
                break;
            }
        }

        UnhookWindowsHookEx(mouse_hook)?;
        UnhookWindowsHookEx(kb_hook)?;
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_PANIC);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_LOCK);
        let _ = ClipCursor(None);
    }
    Ok(())
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let swallow = !PASS_THROUGH.load(Ordering::SeqCst);
        if let Some(tx) = HOOK_TX.get() {
            let info = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
            let ev = match wparam.0 as u32 {
                x if x == WM_MOUSEMOVE as u32 => {
                    let (dx, dy) = if HAVE_LAST_MOUSE.load(Ordering::SeqCst) {
                        let lx = LAST_MOUSE_X.load(Ordering::SeqCst);
                        let ly = LAST_MOUSE_Y.load(Ordering::SeqCst);
                        (info.pt.x - lx, info.pt.y - ly)
                    } else {
                        HAVE_LAST_MOUSE.store(true, Ordering::SeqCst);
                        (0, 0)
                    };
                    LAST_MOUSE_X.store(info.pt.x, Ordering::SeqCst);
                    LAST_MOUSE_Y.store(info.pt.y, Ordering::SeqCst);
                    Some(InputEvent::MouseMove {
                        dx,
                        dy,
                        screen_x: info.pt.x,
                        screen_y: info.pt.y,
                    })
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
                let _ = tx.send(HookMsg::Input(ev));
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
        let swallow = !PASS_THROUGH.load(Ordering::SeqCst);
        if let Some(tx) = HOOK_TX.get() {
            let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let pressed = wparam.0 as u32 != WM_KEYUP as u32;
            if let Some(code) = vk_to_portable(info.vkCode as u16) {
                let ev = InputEvent::Key {
                    code,
                    pressed,
                    modifiers: KeyModifiers::default(),
                };
                let _ = tx.send(HookMsg::Input(ev));
            }
        }
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}
