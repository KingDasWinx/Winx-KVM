//! Bounded context: **Pairing**.
//!
//! Estabelece confiança entre dois devices via PIN de 6 dígitos +
//! troca de chaves X25519 efêmeras, assinadas pela Ed25519 de longo prazo.
//!
//! Ver [docs/PLANNING.md][p] épico 4.
//!
//! [p]: ../../../../../docs/PLANNING.md

pub mod events;
pub mod session;

pub use events::{
    PairingCancelled, PairingCompleted, PairingFailed, PairingIncoming, PairingRequested,
};
pub use session::{PairingRole, PairingSession, PairingState, Pin};
