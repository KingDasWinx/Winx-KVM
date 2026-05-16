//! Bounded context: **DataExchange**.
//!
//! Sincronização de clipboard (texto/imagem/RTF/arquivos) e transferência
//! de arquivos via streams QUIC dedicados (chunks 64KB + SHA-256).
//!
//! Clipboard de texto entra no MVP (v0.1). Imagens/RTF em v0.2.
//! Arquivos via Ctrl+C/V em v0.3 — ver [docs/PLANNING.md][p].
//!
//! [p]: ../../../../../docs/PLANNING.md
