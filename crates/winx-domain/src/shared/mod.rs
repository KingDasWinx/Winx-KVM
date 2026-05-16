//! Tipos que atravessam todos os bounded contexts.

mod error;
mod event;
pub mod ids;

pub use error::{DomainError, DomainErrorCode};
pub use event::DomainEvent;
