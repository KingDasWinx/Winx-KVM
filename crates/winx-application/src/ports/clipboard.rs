use async_trait::async_trait;

/// Handle opaco para sessão de watcher de clipboard.
#[derive(Debug)]
pub struct ClipboardWatcherHandle {
    pub id: u64,
}

#[async_trait]
pub trait ClipboardBackend: Send + Sync + 'static {
    /// Inicia watcher; chama `on_change` quando o texto UTF-8 local muda.
    async fn start_watcher(
        &self,
        poll_ms: u64,
        on_change: Box<dyn Fn(String) + Send + Sync>,
    ) -> anyhow::Result<ClipboardWatcherHandle>;

    async fn stop_watcher(&self, handle: ClipboardWatcherHandle) -> anyhow::Result<()>;

    async fn get_text(&self) -> anyhow::Result<Option<String>>;

    async fn set_text(&self, text: String) -> anyhow::Result<()>;
}
