//! Bounded context: **Transport**.
//!
//! Gerencia conexões QUIC ativas com peers confiáveis, multiplexa streams
//! tipados por tipo de dado (`Control`, `Input`, `Audio`, `Data`).
//!
//! Ver [docs/PLANNING.md][p] épico 5.
//!
//! [p]: ../../../../../docs/PLANNING.md
