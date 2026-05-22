//! Ports — traits que a aplicação precisa do mundo externo.
//!
//! Cada port é implementada por um adapter concreto em `winx-infra`.
//! Convenção: traits são `async` (via `async_trait`) e falhas técnicas
//! inesperadas retornam `anyhow::Result`; erros com semântica de domínio
//! retornam `Result<T, DomainError>`.

pub mod clipboard;
pub mod discovery;
pub mod discovery_query;
pub mod identity;
pub mod input;
pub mod monitor;
pub mod network;
pub mod pairing;
pub mod transport;
pub mod workspace;
pub mod workspace_transport;

pub use clipboard::{ClipboardBackend, ClipboardWatcherHandle};
pub use discovery::{AnnounceInfo, DiscoveryAdapter, DiscoveryEvent, WINX_KVM_PORT};
pub use discovery_query::DiscoveryQuery;
pub use identity::{IdentityStore, SecretStore};
pub use input::{CaptureHandle, InputBackend};
pub use monitor::MonitorBackend;
pub use network::NetworkInterfacesProvider;
pub use pairing::{
    pairing_socket_addr, DecodedPairingMessage, PairingTransport, WINX_KVM_PAIRING_PORT,
};
pub use transport::TransportAdapter;
pub use workspace::WorkspaceStore;
pub use workspace_transport::{
    DecodedWorkspaceInviteMessage, WorkspaceInviteTransport, WORKSPACE_INVITE_PORT,
};
