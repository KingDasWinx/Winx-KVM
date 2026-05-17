//! Inicialização do tracing/logging.
//!
//! Filtro controlado pela env var `WINX_LOG` (formato `tracing-subscriber`).
//! Default em debug: `winx=debug,info`. Default em release: `winx=info,warn`.
//! Logs também são persistidos em arquivo rotativo em %APPDATA%\Winx-KVM\logs\winx-kvm.log.

use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init() {
    let default_filter = if cfg!(debug_assertions) {
        "winx=debug,info"
    } else {
        "winx=info,warn"
    };

    let filter =
        EnvFilter::try_from_env("WINX_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));

    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true);

    let log_dir = {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata)
            .join("br.com.winxkvm.app")
            .join("logs")
    };

    let _ = std::fs::create_dir_all(&log_dir);

    let file_path = log_dir.join("winx-kvm.log");
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open log file {}: {}", file_path.display(), e);
            return;
        }
    };

    let file_layer = fmt::layer()
        .with_writer(std::sync::Arc::new(file))
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();
}
