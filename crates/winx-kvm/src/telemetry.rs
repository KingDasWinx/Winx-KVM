//! Inicialização do tracing/logging.
//!
//! Filtro controlado pela env var `WINX_LOG` (formato `tracing-subscriber`).
//! Default em debug: `winx=debug,info`. Default em release: `winx=info,warn`.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init() {
    let default_filter = if cfg!(debug_assertions) {
        "winx=debug,info"
    } else {
        "winx=info,warn"
    };

    let filter =
        EnvFilter::try_from_env("WINX_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .init();
}
