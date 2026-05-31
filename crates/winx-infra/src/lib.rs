//! Adapters concretos do Winx-KVM.
//!
//! Cada módulo implementa uma ou mais ports definidas em
//! `winx-application::ports`. Esta é a única camada autorizada a importar
//! crates de I/O (`mdns-sd`, `quinn`, `windows`, `cpal`, etc).

// Esta crate é a ÚNICA do workspace que pode usar `unsafe` (Win32 hooks, FFI
// para drivers de áudio). Cada módulo que precisar deve declarar
// `#![allow(unsafe_code)]` localmente e justificar com SAFETY comments.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod clipboard_arboard;
pub mod config_store_toml;
pub mod cursor_trap;
pub mod discovery_mdns;
pub mod discovery_query_memory;
pub mod discovery_query_registry;
pub mod dpi;
pub mod identity_store_toml;
pub mod input_vk_map;
pub mod input_win32;
pub mod monitor_win32;
pub mod network_config;
pub mod network_interfaces;
pub mod network_interfaces_provider;
pub mod network_watcher;
pub mod pairing_udp;
pub mod secret_store_keyring;
pub mod transport_quic;
pub mod workspace_invite_udp;
pub mod workspace_store_toml;

pub use clipboard_arboard::ArboardClipboardBackend;
pub use config_store_toml::TomlConfigStore;
pub use discovery_mdns::MdnsDiscoveryAdapter;
pub use discovery_query_memory::MemoryDiscoveryQuery;
pub use discovery_query_registry::RegistryDiscoveryQuery;
pub use dpi::ensure_per_monitor_v2;
pub use identity_store_toml::TomlIdentityStore;
pub use input_win32::Win32InputBackend;
pub use monitor_win32::Win32MonitorBackend;
pub use network_interfaces_provider::IfAddrsNetworkProvider;
pub use pairing_udp::UdpPairingTransport;
pub use secret_store_keyring::KeyringSecretStore;
pub use transport_quic::{generate_or_load_quic_cert, QuicTransportAdapter};
pub use workspace_invite_udp::UdpWorkspaceInviteTransport;
pub use workspace_store_toml::TomlWorkspaceStore;

// Adapters futuros (sprint a sprint):
// pub mod input_win32;
// pub mod monitor_win32;
// pub mod audio_cpal;
// pub mod clipboard_arboard;
// pub mod clipboard_files_win32;
// pub mod filesystem_std;
