# Rust Dev Build Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduzir o tempo entre `cargo tauri dev` e o app utilizável, atacando link, perfis Cargo, startup runtime (firewall UAC) e workflow de dev split.

**Architecture:** Medir primeiro (`cargo build --timings`), depois aplicar ganhos de baixo risco no repo (`.cargo/config.toml`, `[profile.dev]`, fix de firewall) e só então otimizações estruturais (tokio features). **Não** usar sccache + incremental juntos em dev — manter incremental local; reservar sccache para CI se necessário. Frontend já usa `devUrl` no Tauri 2; foco é Rust + startup.

**Tech Stack:** Rust 1.93.1 stable, `x86_64-pc-windows-msvc`, Tauri 2.11.1, Cargo profiles, rust-lld (LLVM linker incluído no toolchain Rust)

**Estado atual (baseline do repo):**
- Sem `.cargo/config.toml` no projeto nem em `~/.cargo/config.toml`
- `[profile.dev] opt-level = 1` + `[profile.dev.package."*"] opt-level = 3` já existem em `Cargo.toml` raiz
- `tauri.conf.json` já tem `devUrl: http://localhost:5173` e `beforeDevCommand` com Vite
- `tokio` usa `features = ["full"]` no workspace — compila mais do que o projeto usa
- **Bug de startup:** logs mostram UAC de firewall a cada dev run (~10–20s) porque regras por porta retornam `Program: Any` e `needs_fix()` trata como path stale

---

## File Map

| File | Responsibility |
|------|----------------|
| `.cargo/config.toml` | Linker `rust-lld`, flags de build compartilhadas no repo |
| `Cargo.toml` (raiz) | Perfis `dev` / overrides de debug |
| `crates/winx-infra/src/network_config.rs` | Inspeção firewall — corrigir falso positivo em dev |
| `crates/winx-infra/src/network_config.rs` (tests) | Testes unitários de `needs_fix` / `issues` |
| `Cargo.toml` (raiz) `[workspace.dependencies]` | Reduzir features de `tokio` |
| `scripts/dev-backend.ps1` (novo) | Workflow split: Rust watch sem reiniciar Vite |
| `scripts/dev-frontend.ps1` (novo) | Vite isolado |
| `docs/superpowers/plans/2026-05-31-rust-dev-build-baseline.md` (opcional) | Colar timings antes/depois |

---

### Task 1: Baseline — medir onde o tempo vai

**Files:**
- Create: `docs/superpowers/plans/2026-05-31-rust-dev-build-baseline.md` (resultados colados manualmente)

- [ ] **Step 1: Limpar e medir cold build**

```powershell
cd C:\Users\kingdaswinx\Documents\GitHub\Winx-KVM
cargo clean
Measure-Command { cargo build -p winx-kvm } | Select-Object TotalSeconds
```

Anotar segundos em `2026-05-31-rust-dev-build-baseline.md` como **cold build**.

- [ ] **Step 2: Medir rebuild incremental (só touch em crate interno)**

```powershell
(Get-Item crates\winx-application\src\use_cases\workspace.rs).LastWriteTime = Get-Date
Measure-Command { cargo build -p winx-kvm } | Select-Object TotalSeconds
```

Anotar como **incremental rebuild (application change)**.

- [ ] **Step 3: Gerar relatório HTML de timings**

```powershell
cargo build -p winx-kvm --timings
```

Abrir `target\cargo-timings\cargo-timing.html`. Anotar os **5 crates mais lentos** (link + compile) no baseline doc.

- [ ] **Step 4: Medir startup app (sem recompilar)**

```powershell
Measure-Command { cargo run -p winx-kvm --no-default-features } | Select-Object TotalSeconds
```

Rodar **sem** mudar `.rs` entre build e run. Anotar como **startup-only** (inclui firewall UAC se disparar).

Expected: baseline doc preenchido com 4 números + top crates.

---

### Task 2: Corrigir falso positivo do firewall (ganho imediato no startup)

**Problema:** Regras `Winx-KVM mDNS UDP In`, `QUIC`, `Pairing`, `Workspace` usam `-LocalPort` sem `-Program`. O PowerShell retorna `Program = "Any"`. `needs_fix()` compara `"any"` ≠ caminho do `.exe` → **UAC a cada `cargo tauri dev`**.

**Files:**
- Modify: `crates/winx-infra/src/network_config.rs:50-132`
- Modify: `crates/winx-infra/src/network_config.rs` (módulo `#[cfg(test)]`)
- Test: `cargo test -p winx-infra network_config`

- [ ] **Step 1: Write the failing test**

Adicionar no final de `network_config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str, program: Option<&str>) -> FirewallRule {
        FirewallRule {
            name: name.to_string(),
            profile: "Any".to_string(),
            protocol: "UDP".to_string(),
            program: program.map(str::to_string),
            enabled: "True".to_string(),
            direction: "Inbound".to_string(),
        }
    }

    #[test]
    fn needs_fix_ignores_port_rules_with_program_any() {
        let status = NetworkConfigStatus {
            current_exe: PathBuf::from(r"c:\proj\target\debug\winx-kvm.exe"),
            firewall_rules: vec![
                rule("Winx-KVM mDNS UDP In", Some("Any")),
                rule("Winx-KVM QUIC UDP In", Some("Any")),
                rule("Winx-KVM Program UDP In", Some(r"c:\proj\target\debug\winx-kvm.exe")),
                rule("Winx-KVM Program UDP Out", Some(r"c:\proj\target\debug\winx-kvm.exe")),
            ],
            ..Default::default()
        };

        assert!(!status.needs_fix(), "port rules with Program=Any must not force UAC");
    }

    #[test]
    fn needs_fix_flags_stale_program_rule() {
        let status = NetworkConfigStatus {
            current_exe: PathBuf::from(r"c:\proj\target\debug\winx-kvm.exe"),
            firewall_rules: vec![
                rule("Winx-KVM Program UDP In", Some(r"c:\old\winx-kvm.exe")),
                rule("Winx-KVM Program UDP Out", Some(r"c:\proj\target\debug\winx-kvm.exe")),
            ],
            ..Default::default()
        };

        assert!(status.needs_fix());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p winx-infra network_config::tests::needs_fix_ignores_port_rules_with_program_any -- --nocapture
```

Expected: **FAIL** — `needs_fix()` retorna `true`.

- [ ] **Step 3: Fix `needs_fix` and `issues`**

Substituir o loop de validação de program (linhas ~73-82 e ~121-128) por helper:

```rust
fn is_program_scoped_rule(name: &str) -> bool {
    name.starts_with("Winx-KVM Program ")
}

fn program_path_is_stale(rule: &FirewallRule, exe_path: &str) -> bool {
    let Some(prog) = &rule.program else {
        return false;
    };
    if !is_program_scoped_rule(&rule.name) {
        return false;
    }
    let prog_lower = prog.to_lowercase();
    if prog_lower == "any" || prog_lower.is_empty() {
        return false;
    }
    prog_lower != exe_path
}
```

Usar `program_path_is_stale` em `needs_fix()` (retorna `true` se stale) e em `issues()` (push mensagem só para regras program-scoped).

Atualizar `expected_rules` em `needs_fix` e `issues` para incluir `"Winx-KVM Pairing UDP In"` e `"Winx-KVM Workspace UDP In"` (já criadas em `reconfigure()`).

- [ ] **Step 4: Run tests**

```powershell
cargo test -p winx-infra network_config -- --nocapture
```

Expected: **PASS** (todos os testes do módulo).

- [ ] **Step 5: Validar manualmente**

```powershell
cargo run -p winx-kvm --no-default-features
```

Expected: log `firewall já está OK` **sem** prompt UAC quando regras já existem.

- [ ] **Step 6: Commit**

```powershell
git add crates/winx-infra/src/network_config.rs
git commit -m "fix: evitar UAC de firewall a cada dev run por falso positivo em regras por porta"
```

---

### Task 3: Linker rápido — `rust-lld` no projeto

**Referência (Cargo Book — Build Performance):** configurar linker alternativo via `.cargo/config.toml`.

**Files:**
- Create: `.cargo/config.toml`

- [ ] **Step 1: Criar config do linker**

Conteúdo completo de `.cargo/config.toml`:

```toml
# Compartilhado no repo — Windows MSVC (host padrão do Winx-KVM).
[target.x86_64-pc-windows-msvc]
linker = "rust-lld"

# Evita rebuild desnecessário quando variáveis de ambiente mudam sem afetar compilação.
[env]
# Mantém incremental estável (não definir CARGO_INCREMENTAL=0 aqui).
```

- [ ] **Step 2: Verificar que rust-lld existe**

```powershell
rustc -vV
where.exe rust-lld
```

Expected: `where.exe` encontra `rust-lld.exe` no mesmo prefix do toolchain (LLVM 21.x no Rust 1.93).

- [ ] **Step 3: Medir link após clean build**

```powershell
cargo clean
Measure-Command { cargo build -p winx-kvm } | Select-Object TotalSeconds
```

Comparar com baseline Task 1. Anotar delta no doc baseline.

- [ ] **Step 4: Commit**

```powershell
git add .cargo/config.toml
git commit -m "chore: usar rust-lld no Windows para reduzir link time em dev"
```

---

### Task 4: Afinar `[profile.dev]` para compilação mais rápida

**Referência (Cargo Book — Profiles):** defaults dev usam `opt-level = 0`, `incremental = true`, `codegen-units = 256`; debug reduzido acelera compilação.

**Files:**
- Modify: `Cargo.toml:101-106`

- [ ] **Step 1: Substituir bloco de profile dev**

Trocar:

```toml
[profile.dev]
opt-level = 1

# Build mais rápido das dependências mesmo em dev
[profile.dev.package."*"]
opt-level = 3
```

Por:

```toml
[profile.dev]
opt-level = 0
debug = 1              # line tables only — debug suficiente, compila mais rápido
incremental = true
codegen-units = 256

# Deps compiladas com otimização uma vez; só recompilam quando Cargo.lock muda
[profile.dev.package."*"]
opt-level = 3
debug = false

# Build scripts / proc-macros: compilar rápido (Cargo default build-override)
[profile.dev.build-override]
opt-level = 0
codegen-units = 256
debug = false
```

- [ ] **Step 2: Rebuild e testar que app ainda roda**

```powershell
cargo build -p winx-kvm
cargo test -p winx-domain -- --nocapture
cargo run -p winx-kvm --no-default-features
```

Expected: build OK, app abre, testes domain passam.

- [ ] **Step 3: Medir incremental rebuild**

Repetir Step 2 da Task 1 (touch em `workspace.rs`). Comparar com baseline.

- [ ] **Step 4: Commit**

```powershell
git add Cargo.toml
git commit -m "chore: profile dev mais rápido (opt-level 0, debug line-tables, codegen-units 256)"
```

---

### Task 5: Reduzir features de `tokio` (menos código para compilar/recompilar)

**Files:**
- Modify: `Cargo.toml:41-42`
- Test: `cargo test --workspace` (smoke)

- [ ] **Step 1: Write failing compile check list**

Antes de mudar, confirmar features usadas no repo (grep já mapeou): `rt-multi-thread`, `sync`, `time`, `net`, `fs`, `macros`, `io-util` (via quinn/indirect).

- [ ] **Step 2: Substituir tokio full por features mínimas**

Em `Cargo.toml` raiz:

```toml
tokio = { version = "1.40", features = [
  "rt-multi-thread",
  "sync",
  "time",
  "net",
  "fs",
  "macros",
  "io-util",
] }
```

- [ ] **Step 3: Build workspace inteiro**

```powershell
cargo build --workspace
```

Expected: **PASS**. Se falhar com feature missing, adicionar **apenas** a feature indicada na mensagem de erro (ex.: `"signal"` — improvável neste projeto).

- [ ] **Step 4: Run tests críticos**

```powershell
cargo test -p winx-application -p winx-infra -p winx-domain
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml
git commit -m "chore: reduzir features de tokio de full para subset usado pelo workspace"
```

---

### Task 6: sccache — decisão explícita (não ativar em dev local)

**Contexto:** sccache e `CARGO_INCREMENTAL=1` conflitam. Para loop edit-rebuild em `.rs`, incremental local ganha.

**Files:**
- Create: `.cargo/config.toml` (comentário documentando decisão) — ou seção no baseline doc

- [ ] **Step 1: Documentar decisão no baseline doc**

Adicionar seção:

```markdown
## sccache
- Dev local: NÃO usar (incremental ativo)
- CI (opcional futuro): CARGO_INCREMENTAL=0 + rustc-wrapper=sccache em workflow GitHub Actions
```

- [ ] **Step 2: (Opcional) Instalar e validar só para CI**

```powershell
cargo install sccache
sccache --show-stats
```

**Não** adicionar `rustc-wrapper = "sccache"` em `.cargo/config.toml` do dev local neste plano.

- [ ] **Step 3: Commit doc only (se alterou baseline)**

```powershell
git add docs/superpowers/plans/2026-05-31-rust-dev-build-baseline.md
git commit -m "docs: baseline e decisão sccache vs incremental para dev"
```

---

### Task 7: Workflow dev split — Rust watch sem reiniciar Vite

**Contexto:** `tauri.conf.json` já aponta `devUrl` para Vite. Quando só muda frontend, não precisa rebuild Rust. Quando só muda Rust, evitar subir Vite de novo.

**Files:**
- Create: `scripts/dev-frontend.ps1`
- Create: `scripts/dev-backend.ps1`
- Modify: `CLAUDE.md:64-65` (comandos de dev)

- [ ] **Step 1: Criar `scripts/dev-frontend.ps1`**

```powershell
Set-Location $PSScriptRoot\..\ui
pnpm dev
```

- [ ] **Step 2: Criar `scripts/dev-backend.ps1`**

```powershell
Set-Location $PSScriptRoot\..\crates\winx-kvm
# Vite deve já estar rodando em :5173
$env:CARGO_TARGET_DIR = "$PSScriptRoot\..\target"
cargo watch -x "run --no-default-features"
```

Instalar cargo-watch uma vez: `cargo install cargo-watch`

- [ ] **Step 3: Documentar fluxo em CLAUDE.md**

Substituir bloco de dev por:

```powershell
# Terminal 1 — frontend (hot reload UI)
.\scripts\dev-frontend.ps1

# Terminal 2 — backend (recompila só .rs)
.\scripts\dev-backend.ps1

# Fluxo único (como hoje)
cargo tauri dev
```

- [ ] **Step 4: Testar split workflow**

Terminal 1: `.\scripts\dev-frontend.ps1`  
Terminal 2: `.\scripts\dev-backend.ps1`  
Mudar um `.tsx` → só Vite recarrega. Mudar um `.rs` → só cargo-watch rebuilda.

- [ ] **Step 5: Commit**

```powershell
git add scripts/dev-frontend.ps1 scripts/dev-backend.ps1 CLAUDE.md
git commit -m "chore: scripts dev split frontend/backend para iterar mais rápido"
```

---

### Task 8: Validação final e metas

**Files:**
- Modify: `docs/superpowers/plans/2026-05-31-rust-dev-build-baseline.md`

- [ ] **Step 1: Repetir medições da Task 1**

Preencher coluna **After** no baseline doc.

- [ ] **Step 2: Metas de aceite**

| Métrica | Meta |
|---------|------|
| Cold build `-p winx-kvm` | ≥ 25% mais rápido vs baseline |
| Incremental rebuild (touch application) | ≥ 30% mais rápido vs baseline |
| Startup `cargo run` sem recompilar | Sem UAC firewall; < 5s até log `iniciando Winx-KVM` |
| Link (últimos segundos do build) | Redução visível no HTML de timings |

- [ ] **Step 3: Checagens obrigatórias do projeto**

```powershell
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cd ui; pnpm tsc --noEmit; pnpm lint
```

Expected: tudo verde antes de considerar concluído.

---

## Fora de escopo (YAGNI neste ciclo)

| Item pesquisado | Motivo para adiar |
|-----------------|-------------------|
| Feature flags (`--no-default-features`) | Nenhum `[features]` existe nos crates hoje; custo alto, ganho incerto |
| mold (Linux) | Host alvo é Windows MSVC |
| trocar `devUrl` / Vite | Já configurado corretamente em `tauri.conf.json` |
| sccache em dev local | Conflita com incremental; pior para loop Rust |

---

## Self-Review

| Requisito do usuário | Task |
|----------------------|------|
| rust-lld linker | Task 3 |
| sccache | Task 6 (decisão: não em dev) |
| profile dev + deps opt-level | Task 4 (expande o que já existia) |
| feature flags dev | Fora de escopo |
| devUrl / frontend hot reload | Task 7 (workflow split) |
| cargo-watch | Task 7 |
| cargo build --timings | Task 1 + 8 |
| Comandos sem prefixo `rtk` | Todos usam `cargo`/`pnpm` normais |

**Gap fechado além da pesquisa:** Task 2 (firewall UAC) — evidência direta nos logs do usuário.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-31-rust-dev-build-optimization.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
