//! Tauri commands expostos ao frontend via `invoke()`.
//!
//! Convenção: handlers ficam thin — apenas chamam o use case correspondente
//! em `winx-application` e mapeiam o resultado para um DTO serializável.
//! Erros são serializados com seu `DomainErrorCode` (string estável) para
//! permitir tradução i18n no frontend.

mod app;
mod clipboard;
mod discovery;
mod firewall;
mod identity;
mod kvm_layout;
mod input_control;
mod lab;
mod monitor_layout_dto;
mod pairing;
mod settings;
mod transport;
mod workspace;

pub use app::*;
pub use clipboard::*;
pub use discovery::*;
pub use firewall::*;
pub use identity::*;
pub use kvm_layout::*;
pub use input_control::*;
pub use lab::*;
pub use monitor_layout_dto::{layout_to_dto, MonitorLayoutDto};
pub use pairing::*;
pub use settings::*;
pub use transport::*;
pub use workspace::*;
