//! Camada de aplicação do Winx-KVM.
//!
//! - **Use cases**: comandos e queries que orquestram o domínio.
//! - **Ports**: traits que o domínio precisa do mundo externo (storage,
//!   transport, input, audio, clipboard, filesystem). Implementações vivem
//!   em `winx-infra`.
//! - **Event bus**: canal `tokio::broadcast` por onde `DomainEvent` circula
//!   entre bounded contexts.

#![forbid(unsafe_code)]

pub mod bus;
pub mod ports;
pub mod use_cases;

pub use bus::EventBus;
