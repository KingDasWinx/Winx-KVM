//! Camada de domínio do Winx-KVM.
//!
//! Esta crate é **pura**: nada de I/O, rede, filesystem ou APIs do SO.
//! Define entidades, value objects, agregados e eventos de domínio organizados
//! em 7 bounded contexts. Veja [README §"Modelo de Domínio"][readme] para
//! responsabilidades de cada um.
//!
//! [readme]: ../../../../README.md

#![forbid(unsafe_code)]

pub mod shared;

pub mod data_exchange;
pub mod discovery;
pub mod identity;
pub mod input_control;
pub mod media;
pub mod pairing;
pub mod transport;
pub mod workspace;

pub use shared::{DomainError, DomainEvent};
