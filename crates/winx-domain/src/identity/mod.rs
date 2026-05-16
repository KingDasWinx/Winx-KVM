//! Bounded context: **Identity**.
//!
//! Identidade criptográfica do device local e lista de peers confiáveis.
//!
//! Agregados / entidades planejados (ver [docs/PLANNING.md][p] épico 2):
//! - `Device` (raiz): id, username, par de chaves Ed25519, criado em.
//! - `TrustedPeer`: id remoto, public key, username, paired_at, last_seen.
//!
//! Value objects: `DeviceId` (em [`shared::ids`]), `PublicKey` (32 bytes),
//! `Fingerprint` (hash SHA-256 truncado para exibição).
//!
//! [p]: ../../../../../docs/PLANNING.md
//! [`shared::ids`]: crate::shared::ids
