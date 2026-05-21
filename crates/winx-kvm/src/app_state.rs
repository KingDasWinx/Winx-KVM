//! Estado global compartilhado entre Tauri commands.
//!
//! `AppState` carrega o event bus. Estados por bounded context (`IdentityState`,
//! etc.) são adicionados via `app.manage()` no `setup()` para manter o
//! acoplamento zero entre contextos.

use std::sync::Arc;
use tokio::sync::Mutex;

use winx_application::{
    ports::IdentityStore, ClipboardService, ConnectionLabService, DiscoveryService, EnsureDevice,
    EventBus, InputControlService, PairingService, TransportService,
};

/// Estado compartilhado entre todos os commands (event bus global).
#[derive(Debug, Clone)]
pub struct AppState {
    pub bus: EventBus,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            bus: EventBus::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Estado do bounded context Identity.
///
/// Injetado no Tauri managed state durante o `setup()` com as implementações
/// concretas dos ports (`TomlIdentityStore`, `KeyringSecretStore`).
pub struct IdentityState {
    pub ensure_device: Arc<EnsureDevice>,
    pub identity_store: Arc<dyn IdentityStore>,
}

/// Estado do bounded context Discovery.
pub struct DiscoveryState {
    pub discovery: Arc<DiscoveryService>,
}

/// Estado do bounded context Pairing.
pub struct PairingState {
    pub pairing: Arc<PairingService>,
}

/// Estado do bounded context Transport.
pub struct TransportState {
    pub transport: Arc<TransportService>,
}

/// Estado do bounded context InputControl.
pub struct InputControlState {
    pub input_control: Arc<InputControlService>,
}

/// Estado do bounded context DataExchange (clipboard).
pub struct ClipboardState {
    pub clipboard: Arc<ClipboardService>,
}

/// Estado da aba Lab (diagnóstico).
pub struct LabState {
    pub lab: Arc<ConnectionLabService>,
}

/// Estado de configuração de firewall.
pub struct FirewallState {
    pub is_configured: Arc<Mutex<bool>>,
}
