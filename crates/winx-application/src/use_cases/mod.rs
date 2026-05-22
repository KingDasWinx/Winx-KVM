//! Use cases — comandos e queries da aplicação, organizados por bounded context.

pub mod clipboard;
pub mod connection_lab;
pub mod discovery;
pub mod identity;
pub mod input_control;
pub mod input_streams;
pub mod mouse_coalesce;
pub mod pairing;
pub mod transport;
pub mod update_device_profile;
pub mod workspace;

pub use clipboard::ClipboardService;
pub use connection_lab::ConnectionLabService;
pub use discovery::DiscoveryService;
pub use identity::EnsureDevice;
pub use input_control::InputControlService;
pub use pairing::PairingService;
pub use transport::TransportService;
pub use update_device_profile::UpdateDeviceProfile;
pub use workspace::WorkspaceService;
