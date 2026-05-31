# Workspace W3/W4 Audit Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corrigir gaps e bugs encontrados na auditoria de Sprints W3 e W4 contra `Workspace-TODO.md`.

**Architecture:** Fixes mínimos no backend (eventos de sync remotos, cursor local, validação de membership, warp multi-monitor) e frontend (disponibilidade, toast, leave/forget no drawer). W4.6–W4.8 já existem no código; W4.9–W4.13 permanecem parciais/manuais.

**Tech Stack:** Rust (winx-domain, winx-application, winx-infra, winx-kvm), React 19 + Mantine 9, i18next

**Status:** ✅ Executado 2026-05-30 (subagent-driven-development)

---

## Audit Summary

| Área | OK | Fix needed |
|------|-----|------------|
| W3.1–W3.8 | Backend + UI core | — |
| W3.9 | ✅ | Lógica `isAvailable` corrigida |
| W3.11 | ✅ | Leave no drawer |
| W3.12 | ✅ | Teste split-brain adicionado |
| W3.13 | ✅ | Toast remoto + nome |
| W3.14 | ✅ | Drawer forget gated por `is_orphan` |
| W4.1–W4.5 | ✅ | B1/B3/B4 corrigidos |
| W4.6–W4.8 | Implementados | Hotkey não configurável (aceitável MVP) |
| W4.9–W4.13 | Teste unitário cursor | Smoke multi-PC manual |

---

### Task 1: Cursor — evento local + validação de membership

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [x] **Step 1:** Após `update_local` em `publish_global_cursor`, publicar `WorkspaceGlobalCursorMoved` no bus
- [x] **Step 2:** Em `handle_global_cursor`, rejeitar se `sender_device_id`/`sender_pubkey` ∉ `ws.members`
- [x] **Step 3:** Run `cargo test -p winx-application use_cases::workspace::tests` → **22 passed**

---

### Task 2: Sync toast — só remoto + nome do workspace

**Files:**
- Modify: `crates/winx-domain/src/workspace/events.rs`
- Modify: `crates/winx-application/src/use_cases/workspace.rs`
- Modify: `crates/winx-kvm/src/events/mod.rs`
- Modify: `ui/src/ipc/events.ts`
- Modify: `ui/src/components/workspace/WorkspacesPanel.tsx`
- Modify: `ui/src/i18n/locales/en/workspace.json`, `ui/src/i18n/locales/pt-BR/workspace.json`

- [x] **Step 1:** Adicionar `workspace_name: String` e `from_remote: bool` a `WorkspaceSyncApplied`
- [x] **Step 2:** `update_workspace` → `from_remote: false`; `handle_workspace_sync` → `from_remote: true`
- [x] **Step 3:** Mapear `sync_from_remote` + `workspace_name` no `FrontendEvent`
- [x] **Step 4:** Toast só quando `sync_from_remote === true`, mensagem `"{{name}} was updated"`

---

### Task 3: UI W3.9 / W3.11 / W3.14

**Files:**
- Modify: `ui/src/components/workspace/WorkspaceCard.tsx`
- Modify: `ui/src/components/workspace/WorkspaceDetailDrawer.tsx`
- Modify: `ui/src/i18n/locales/en/workspace.json`, `pt-BR/workspace.json`

- [x] **Step 1:** `isAvailable` = qualquer chave `presence[workspaceId:*] === true`, ou original sem dados de presença
- [x] **Step 2:** Drawer: Leave (mirror !orphan), Forget (mirror orphan), Delete (original)

---

### Task 4: Split-brain test W3.12

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs` (tests)

- [x] **Step 1:** Teste `split_brain_lww_resolves_to_higher_version` — versão 5 → sync 8 → sync 6 descartado

---

### Task 5: Warp multi-monitor B3

**Files:**
- Modify: `crates/winx-infra/src/input_win32.rs`

- [x] **Step 1:** `warp_cursor_signed` usa `SM_XVIRTUALSCREEN` / `SM_CXVIRTUALSCREEN` para normalização

---

### Task 6: Atualizar Workspace-TODO.md

- [x] Refletir auditoria, bugs corrigidos, status W4.6–W4.8 confirmados no código

---

## Verification (2026-05-30)

```
cargo test -p winx-application use_cases::workspace::tests  → 22 passed
cargo test -p winx-infra workspace_invite_udp::tests        → 7 passed
cargo check -p winx-kvm                                     → OK
pnpm tsc --noEmit                                           → OK
pnpm lint                                                   → BLOCKED (@eslint/js missing in ui/node_modules)
```

**Remaining manual:** W4.13 smoke 2 PCs físicos (fora do escopo deste plano).
