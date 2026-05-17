//! Use cases — comandos e queries da aplicação, organizados por bounded context.

pub mod clipboard;
pub mod discovery;
pub mod identity;
pub mod input_control;
pub mod pairing;
pub mod transport;
pub mod update_device_profile;

pub use clipboard::ClipboardService;
pub use discovery::DiscoveryService;
pub use identity::EnsureDevice;
pub use input_control::InputControlService;
pub use pairing::PairingService;
pub use transport::TransportService;
pub use update_device_profile::UpdateDeviceProfile;
