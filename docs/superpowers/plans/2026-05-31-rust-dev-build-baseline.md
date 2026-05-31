# Rust Dev Build — Baseline & After

Medições em 2026-05-31, host `x86_64-pc-windows-msvc`, Rust 1.93.1.

## Before (logs / plano pré-otimização)

| Métrica | Valor |
|---------|-------|
| Cold build `-p winx-kvm` | ~100–163s (logs anteriores: 1m40s cold, 51s incremental pós-change) |
| Incremental rebuild (touch application) | ~51s |
| Startup UAC firewall | ~10–20s a cada `cargo tauri dev` (falso positivo `Program=Any`) |

## After (pós Tasks 2–7)

| Métrica | Valor |
|---------|-------|
| Full rebuild pós mudança de profile/tokio | 323s (5m23s — esperado: invalidou cache de deps) |
| Incremental rebuild (touch application) | **~9.4s** (~82% vs baseline) |
| Incremental build sem mudanças | **1.1s** |
| `cargo test -p winx-infra network_config` | 2 passed |
| `cargo test -p winx-domain -p winx-application -p winx-infra` | OK |

## sccache

- **Dev local:** NÃO usar (`CARGO_INCREMENTAL=1` + incremental ativo)
- **CI (futuro):** `CARGO_INCREMENTAL=0` + `rustc-wrapper = "sccache"` no workflow GitHub Actions

## Mudanças aplicadas

1. `network_config.rs` — `program_path_is_stale` (sem UAC por regras de porta)
2. `.cargo/config.toml` — `linker = "rust-lld"`
3. `Cargo.toml` — `[profile.dev]` otimizado + tokio features mínimas
4. `scripts/dev-frontend.ps1` + `scripts/dev-backend.ps1` + `CLAUDE.md`

## Próximo rebuild frio

Após cache estabilizar, rodar `cargo clean; Measure-Command { cargo build -p winx-kvm }` para comparar cold build com rust-lld vs baseline histórica.
