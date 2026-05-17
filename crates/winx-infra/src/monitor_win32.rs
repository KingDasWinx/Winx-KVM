//! Enumera monitores via `EnumDisplayMonitors`.

#![allow(unsafe_code)]

use async_trait::async_trait;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use winx_application::ports::MonitorBackend;
use winx_domain::input_control::{MonitorId, MonitorRect};

pub struct Win32MonitorBackend;

struct CollectCtx {
    monitors: Vec<MonitorRect>,
    next_id: u32,
}

unsafe extern "system" fn enum_proc(
    _hmon: HMONITOR,
    _hdc: HDC,
    lprc: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: lparam aponta para CollectCtx válido durante EnumDisplayMonitors.
    let ctx = unsafe { &mut *(lparam.0 as *mut CollectCtx) };
    if lprc.is_null() {
        return BOOL(1);
    }
    let rc = unsafe { *lprc };
    let id = MonitorId(ctx.next_id);
    ctx.next_id += 1;
    let w = rc.right.saturating_sub(rc.left);
    let h = rc.bottom.saturating_sub(rc.top);
    ctx.monitors.push(MonitorRect {
        id,
        x: rc.left,
        y: rc.top,
        width: u32::try_from(w).unwrap_or(0),
        height: u32::try_from(h).unwrap_or(0),
    });
    BOOL(1)
}

#[async_trait]
impl MonitorBackend for Win32MonitorBackend {
    async fn enumerate_local_monitors(&self) -> anyhow::Result<Vec<MonitorRect>> {
        let mut ctx = CollectCtx {
            monitors: Vec::new(),
            next_id: 1,
        };
        // SAFETY: callback e lparam válidos; ctx vive durante a chamada.
        // SAFETY: callback e contexto válidos durante a enumeração.
        let ok = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(enum_proc),
                LPARAM(std::ptr::addr_of_mut!(ctx) as isize),
            )
        };
        if !ok.as_bool() {
            anyhow::bail!("EnumDisplayMonitors falhou");
        }
        if ctx.monitors.is_empty() {
            ctx.monitors.push(MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            });
        }
        Ok(ctx.monitors)
    }
}
