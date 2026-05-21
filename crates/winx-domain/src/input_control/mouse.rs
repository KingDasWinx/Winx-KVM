//! Constantes de filtro/coalesce para movimento de mouse.

/// Soma mínima de |dx|+|dy| para enviar ou injetar (filtra micro-jitter do clip).
pub const MOUSE_SEND_MIN_MANHATTAN: i32 = 2;

/// Intervalo alvo entre flushes agregados no stream Input (ms).
pub const MOUSE_COALESCE_FLUSH_MS: u64 = 8;
