//! Ports — traits que o domínio precisa do mundo externo.
//!
//! Cada port é implementada por um adapter concreto em `winx-infra`.
//! Convenção: traits são `async` (`async_trait`) e operações que falham
//! retornam `Result<T, DomainError>` quando o erro tem semântica de domínio,
//! ou `anyhow::Result<T>` para falhas técnicas inesperadas.
//!
//! Ports serão adicionadas sprint a sprint conforme [docs/PLANNING.md][p].
//!
//! [p]: ../../../../../docs/PLANNING.md
