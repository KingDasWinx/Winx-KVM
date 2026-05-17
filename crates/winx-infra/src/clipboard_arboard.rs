//! Adapter de clipboard via `arboard` com polling (Windows sem evento confiável).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arboard::Clipboard;
use async_trait::async_trait;
use tracing::warn;
use winx_application::ports::{ClipboardBackend, ClipboardWatcherHandle};

struct WatcherState {
    stop: Arc<AtomicBool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

pub struct ArboardClipboardBackend {
    next_handle: AtomicU64,
    watchers: Mutex<std::collections::HashMap<u64, WatcherState>>,
}

impl Default for ArboardClipboardBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ArboardClipboardBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            watchers: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn read_text_blocking() -> anyhow::Result<Option<String>> {
        let mut clipboard = Clipboard::new()?;
        match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn write_text_blocking(text: String) -> anyhow::Result<()> {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(text)?;
        Ok(())
    }
}

#[async_trait]
impl ClipboardBackend for ArboardClipboardBackend {
    async fn start_watcher(
        &self,
        poll_ms: u64,
        on_change: Box<dyn Fn(String) + Send + Sync>,
    ) -> anyhow::Result<ClipboardWatcherHandle> {
        let id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);
        let interval = Duration::from_millis(poll_ms.max(50));
        let last_seen = Arc::new(Mutex::new(String::new()));

        let join = tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                if stop_bg.load(Ordering::SeqCst) {
                    break;
                }
                match tokio::task::spawn_blocking(Self::read_text_blocking).await {
                    Ok(Ok(Some(text))) => {
                        let mut guard = last_seen.lock().unwrap();
                        if *guard != text {
                            guard.clone_from(&text);
                            on_change(text);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(err)) => {
                        warn!(?err, "falha ao ler clipboard");
                    }
                    Err(err) => {
                        warn!(?err, "spawn_blocking clipboard read falhou");
                    }
                }
            }
        });

        self.watchers.lock().unwrap().insert(
            id,
            WatcherState {
                stop,
                join: Some(join),
            },
        );

        Ok(ClipboardWatcherHandle { id })
    }

    async fn stop_watcher(&self, handle: ClipboardWatcherHandle) -> anyhow::Result<()> {
        let state = self.watchers.lock().unwrap().remove(&handle.id);
        if let Some(mut state) = state {
            state.stop.store(true, Ordering::SeqCst);
            if let Some(join) = state.join.take() {
                let _ = join.await;
            }
        }
        Ok(())
    }

    async fn get_text(&self) -> anyhow::Result<Option<String>> {
        tokio::task::spawn_blocking(Self::read_text_blocking).await?
    }

    async fn set_text(&self, text: String) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || Self::write_text_blocking(text)).await?
    }
}
