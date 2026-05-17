//! Ports — traits que a aplicação precisa do mundo externo.
//!
//! Cada port é implementada por um adapter concreto em `winx-infra`.
//! Convenção: traits são `async` (via `async_trait`) e falhas técnicas
//! inesperadas retornam `anyhow::Result`; erros com semântica de domínio
//! retornam `Result<T, DomainError>`.

pub mod clipboard;
pub mod discovery;
pub mod identity;
pub mod input;
pub mod monitor;
pub mod transport;

pub use clipboard::{ClipboardBackend, ClipboardWatcherHandle};
pub use discovery::{AnnounceInfo, DiscoveryAdapter, DiscoveryEvent, WINX_KVM_PORT};
pub use identity::{IdentityStore, SecretStore};
pub use input::{CaptureHandle, InputBackend};
pub use monitor::MonitorBackend;
pub use transport::TransportAdapter;
