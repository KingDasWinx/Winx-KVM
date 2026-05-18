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

    /// Transição segura para estado remoto (HideSystem + Warp + Clip).
    /// Sequência: Hide (não-crítico) → Warp (crítico) → Clip (crítico)
    async fn transition_to_remote(&self, center_x: i32, center_y: i32) -> anyhow::Result<()> {
        // 1. Ocultar cursor globalmente — falha é não-crítica (só cosmética)
        if let Err(e) = self.hide_cursor_system().await {
            tracing::warn!(?e, "falha ao ocultar cursor (não-crítico, continuando)");
        }
        // 2. Warp com assinatura — crítico: sem warp, cursor fica preso na borda
        self.warp_cursor_signed(center_x, center_y).await?;
        // 3. Clip a um retângulo 4x4 — crítico: sem clip, cursor pode escapar
        self.set_cursor_clipped(Some((center_x - 2, center_y - 2, 4, 4))).await?;
        self.reset_mouse_delta_baseline();
        Ok(())
    }

    /// Transição segura para estado local (Release + Restore + Warp + PASS_THROUGH).
    /// Sequência: Clip(None) → Restore → Warp(afastado) → PASS_THROUGH=true
    async fn transition_to_local(&self, edge_x: i32, primary_center_x: i32, primary_center_y: i32) -> anyhow::Result<()> {
        // 1. Liberar confinamento primeiro
        self.set_cursor_clipped(None).await?;
        // 2. Restaurar cursor visual
        self.restore_cursor_system().await?;
        // 3. Warp para longe da borda para evitar re-trigger
        let safe_x = if edge_x > primary_center_x {
            edge_x - 10
        } else {
            edge_x + 10
        };
        self.warp_cursor_signed(safe_x, primary_center_y).await?;
        // 4. PASS_THROUGH será setado em switch_focus
        self.reset_mouse_delta_baseline();
        Ok(())
    }

    /// Oculta cursor globalmente via SetSystemCursor (não afetado por foco de janela)
    async fn hide_cursor_system(&self) -> anyhow::Result<()>;

    /// Restaura cursor via SystemParametersInfoW
    async fn restore_cursor_system(&self) -> anyhow::Result<()>;

    /// Warp com assinatura KVM para escapar do hook (0xDEADC0DE)
    async fn warp_cursor_signed(&self, x: i32, y: i32) -> anyhow::Result<()>;
}
