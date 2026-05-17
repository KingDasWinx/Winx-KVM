//! Bounded context: **Identity**.
//!
//! Identidade criptográfica do device local e lista de peers confiáveis.
//!
//! - `Device`: agregado raiz (id, username, public key Ed25519, created_at).
//! - `TrustedPeer`: peer remoto que completou o pareamento.
//! - `PublicKey`: 32 bytes Ed25519 com fingerprint SHA-256 para exibição.

pub mod device;
pub mod events;
pub mod key;

pub use device::{Device, TrustedPeer};
pub use key::{Fingerprint, PublicKey};
