//! Enum-soma de todos os eventos de domínio.
//!
//! `DomainEvent` é o que circula no event bus interno (`tokio::broadcast`)
//! configurado em `winx-application::bus`. Cada bounded context define seu
//! próprio sub-enum em `<context>/events.rs` e re-exporta as variantes aqui.
//!
//! Implementações concretas dos eventos virão sprint a sprint conforme o
//! backlog em [docs/PLANNING.md][p].
//!
//! [p]: ../../../../../docs/PLANNING.md

use serde::{Deserialize, Serialize};

/// União de todos os eventos de domínio.
///
/// Variantes serão adicionadas sprint a sprint. Por enquanto serve apenas
/// como placeholder estrutural para o event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DomainEvent {
    // Sprint 1 (Identity) — adicionar variantes em F2.x
    // DeviceCreated(identity::events::DeviceCreated),
    // PeerTrusted(identity::events::PeerTrusted),
    //
    // Sprint 2 (Discovery)
    // PeerAppeared(discovery::events::PeerAppeared),
    //
    // ... e assim por diante.
    /// Placeholder enquanto nenhum evento concreto foi modelado ainda.
    /// Será removido quando a primeira variante real for adicionada.
    #[doc(hidden)]
    Placeholder,
}
