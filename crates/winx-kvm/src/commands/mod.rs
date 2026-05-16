//! Tauri commands expostos ao frontend via `invoke()`.
//!
//! Convenção: handlers ficam thin — apenas chamam o use case correspondente
//! em `winx-application` e mapeiam o resultado para um DTO serializável.
//! Erros são serializados com seu `DomainErrorCode` (string estável) para
//! permitir tradução i18n no frontend.

mod app;

pub use app::*;
