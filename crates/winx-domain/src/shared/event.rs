//! Enum-soma de todos os eventos de domínio.
//!
//! Circula no event bus interno (`tokio::broadcast`) em `winx-application::bus`.
//! Cada bounded context define sub-eventos em `<context>/events.rs` e
//! re-exporta as variantes aqui. O frontend recebe versões simplificadas via
//! `FrontendEvent` em `winx-kvm::events`.

use serde::{Deserialize, Serialize};

use crate::data_exchange::events::{ClipboardChanged, ClipboardReceived};
use crate::discovery::events::{PeerAppeared, PeerDisappeared, PeerUpdated};
use crate::identity::events::{DeviceCreated, PeerForgotten};
use crate::input_control::events::{FocusSwitched, HotkeyTriggered, InputBlocked};
use crate::pairing::events::{
    PairingCancelled, PairingCompleted, PairingFailed, PairingIncoming, PairingRequested,
};
use crate::transport::events::{ConnectionEstablished, ConnectionLost, StatsUpdated};

/// União de todos os eventos de domínio.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DomainEvent {
    // --- Identity (Sprint 1) ---
    DeviceCreated(DeviceCreated),
    PeerForgotten(PeerForgotten),

    // --- Discovery (Sprint 2) ---
    PeerAppeared(PeerAppeared),
    PeerDisappeared(PeerDisappeared),
    PeerUpdated(PeerUpdated),

    // --- Pairing (Sprint 3) ---
    PairingRequested(PairingRequested),
    PairingIncoming(PairingIncoming),
    PairingCompleted(PairingCompleted),
    PairingCancelled(PairingCancelled),
    PairingFailed(PairingFailed),

    // --- Transport (Sprint 4) ---
    ConnectionEstablished(ConnectionEstablished),
    ConnectionLost(ConnectionLost),
    StatsUpdated(StatsUpdated),

    // --- InputControl (Sprint 5) ---
    FocusSwitched(FocusSwitched),
    HotkeyTriggered(HotkeyTriggered),
    InputBlocked(InputBlocked),

    // --- DataExchange (Sprint 6) ---
    ClipboardChanged(ClipboardChanged),
    ClipboardReceived(ClipboardReceived),
}
