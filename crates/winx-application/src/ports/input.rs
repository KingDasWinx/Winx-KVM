use async_trait::async_trait;
use winx_domain::input_control::{HotkeyAction, InputEvent};

/// Handle opaco para uma sessão de captura ativa.
#[derive(Debug)]
pub struct CaptureHandle {
    pub id: u64,
}

#[async_trait]
pub trait InputBackend: Send + Sync + 'static {
    /// Inicia captura; eventos chegam via `on_event` (thread de hook).
    async fn start_capture(
        &self,
        on_event: Box<dyn Fn(InputEvent) + Send + Sync>,
        on_hotkey: Box<dyn Fn(HotkeyAction) + Send + Sync>,
    ) -> anyhow::Result<CaptureHandle>;

    async fn stop_capture(&self, handle: CaptureHandle) -> anyhow::Result<()>;

    async fn inject(&self, event: InputEvent) -> anyhow::Result<()>;

    /// `None` libera o cursor; `Some((x,y,w,h))` aplica ClipCursor.
    async fn set_cursor_clipped(&self, rect: Option<(i32, i32, u32, u32)>) -> anyhow::Result<()>;

    /// Define se o hook deve repassar eventos ao Windows (`true`) ou engolir (`false`).
    fn set_pass_through(&self, pass_through: bool);

    /// Move o cursor para coordenadas de tela absolutas.
    async fn warp_cursor(&self, x: i32, y: i32) -> anyhow::Result<()>;

    /// Ignora o delta do próximo movimento do mouse (após clip/warp).
    fn reset_mouse_delta_baseline(&self);

    /// Mostra (`true`) ou oculta (`false`) o cursor do Windows.
    async fn set_cursor_visible(&self, visible: bool) -> anyhow::Result<()>;
}
