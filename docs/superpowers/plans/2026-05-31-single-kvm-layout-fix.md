# Single KVM Layout Sync + Mouse Crossing Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corrigir sync de monitores no modo Single Connection (mostrar N monitores reais do peer, não 1 fake) e eliminar bounce imediato do mouse ao cruzar borda.

**Architecture:** Layout KVM trafega no **mesmo stream QUIC `Data`** do clipboard (`PeerMonitorsAnnounce` + `KvmLayoutShare`, protocolo v6). Um único lado abre outbound; o inbound faz `wait_inbound_stream`. `open_connection` habilita clipboard **antes** de input, depois anuncia monitores. Detecção de retorno usa `placed_remote_bounds()` com clamp `0..=h-1` para evitar falso positivo na borda de entrada.

**Tech Stack:** Rust (winx-domain/application/infra/kvm), winx-protocol v6, Tauri 2, React 19 + Mantine 9, QUIC Data stream compartilhado.

**Diagnóstico (logs 2026-05-31):**
- 3 streams `Data` abertos por conexão (clipboard + layout sync duplicado) → inbound nunca consumia frames de layout.
- `layout sync stream encerrado` sem `monitores do peer recebidos via sync`.
- Bounce ~500ms: `remote_entry_y=1060`, clamp permitia `y=1080`, `should_return_to_local(Bottom)` dispara em `y >= 1078`.
- UI: `buildDefaultLayout` criava 1 monitor remoto fake quando `getPeerMonitors` vazio → editor mostrava 1 local + 1 remoto em vez de 2+1.

**Nota Context7:** MCP Context7 indisponível nesta sessão; análise feita via codebase + logs. Padrão QUIC “single bidirectional stream multiplexed by frame type” alinha com uso existente de `Frame` + `Payload` enum no projeto.

---

## File Structure

| Arquivo | Responsabilidade |
|---------|------------------|
| `crates/winx-protocol/src/lib.rs` | `PROTOCOL_VERSION=6`, `PeerMonitorsAnnounce`, `KvmLayoutShare` |
| `crates/winx-application/src/use_cases/clipboard.rs` | Stream Data único; handler layout; `send_data_payload` |
| `crates/winx-application/src/use_cases/kvm_layout_sync.rs` | Deps, announce, broadcast, persistência peer monitors |
| `crates/winx-application/src/use_cases/input_control.rs` | Wiring deps, clamp cursor, `announce_layout_sync`, save broadcast |
| `crates/winx-kvm/src/lib.rs` | `init_layout_sync` no startup |
| `crates/winx-kvm/src/commands/transport.rs` | Ordem `connect → clipboard → input → announce` |
| `crates/winx-domain/src/input_control/edge.rs` | `should_return_to_local` via `placed_remote_bounds` |
| `ui/src/lib/monitorLayoutGeometry.ts` | Sem monitor remoto fake quando sync pendente |
| `ui/src/components/shared/MonitorLayoutModal.tsx` | Banner sync pendente |

---

### Task 1: Unificar layout no stream Data do clipboard

**Files:**
- Modify: `crates/winx-application/src/use_cases/clipboard.rs`
- Modify: `crates/winx-application/src/use_cases/kvm_layout_sync.rs`

- [x] **Step 1: Handler layout no recv do clipboard**

```rust
// clipboard.rs — dentro do loop recv do stream Data
Payload::PeerMonitorsAnnounce(_) | Payload::KvmLayoutShare(_) => {
    if let Some(handler) = layout_handler.lock().await.as_ref() {
        handler(peer_id, layout_payload.clone());
    }
}
```

- [x] **Step 2: Inbound peer usa wait_inbound_stream (não abre 2º Data)**

```rust
let (tx, mut rx) = if self.transport.is_peer_outbound(peer_id).await {
    self.transport.open_stream_for_peer(peer_id, StreamKind::Data).await?
} else {
    self.transport.wait_inbound_stream(peer_id, StreamKind::Data, DATA_STREAM_WAIT).await?
        .ok_or_else(|| DomainError::new(...))?
};
```

- [x] **Step 3: Remover `start_kvm_layout_sync` (stream separado)**

Run: `rg start_kvm_layout_sync crates/`
Expected: nenhuma ocorrência

---

### Task 2: Wiring startup + open_connection

**Files:**
- Modify: `crates/winx-kvm/src/lib.rs`
- Modify: `crates/winx-kvm/src/commands/transport.rs`
- Modify: `crates/winx-application/src/use_cases/input_control.rs`

- [x] **Step 1: init_layout_sync no startup**

```rust
// lib.rs — após attach_kvm_layout_store
rt.block_on(async {
    input_control.init_layout_sync(Arc::clone(&clipboard)).await;
});
```

- [x] **Step 2: open_connection ordem correta**

```rust
transport.connect_peer(pid, None).await?;
clipboard.enable_for_peer(pid).await?;
input.enable_for_peer(pid).await?;
input.announce_layout_sync(pid).await?;
```

Run: `cargo build -p winx-kvm`
Expected: `Finished` sem erros

---

### Task 3: Fix mouse bounce na borda

**Files:**
- Modify: `crates/winx-application/src/use_cases/input_control.rs` (~1030)
- Modify: `crates/winx-domain/src/input_control/edge.rs`
- Test: `crates/winx-domain/src/input_control/edge.rs` (mod tests)

- [x] **Step 1: Write the failing test**

```rust
#[test]
fn return_does_not_trigger_at_bottom_entry_inset() {
    // remote acima do local → local_exit Top, remote_entry Bottom
    // entry_y = 1080 - REMOTE_ENTRY_INSET_PX = 1060
    assert!(!should_return_to_local(est(1060), &layout));
    assert!(should_return_to_local(est(1078), &layout));
}
```

- [x] **Step 2: Run test to verify it fails (before fix)**

Run: `cargo test -p winx-domain return_does_not_trigger_at_bottom_entry_inset -- --nocapture`
Expected: FAIL (clamp ou bounds errados)

- [x] **Step 3: Clamp com placed_remote_bounds**

```rust
let remote_bounds = layout_data.placed_remote_bounds();
let max_x = remote_bounds.width as i32 - 1;
let max_y = remote_bounds.height as i32 - 1;
let new_x = (old_x + scaled_dx).clamp(0, max_x);
let new_y = (old_y + scaled_dy).clamp(0, max_y);
```

- [x] **Step 4: should_return_to_local usa placed_remote_bounds**

```rust
let remote = layout.placed_remote_bounds();
let w = remote.width as i32;
let h = remote.height as i32;
```

- [x] **Step 5: Run tests**

Run: `cargo test -p winx-domain input_control::edge -- --nocapture`
Expected: 7 passed

---

### Task 4: UI — não inventar monitor remoto

**Files:**
- Modify: `ui/src/lib/monitorLayoutGeometry.ts`
- Modify: `ui/src/components/shared/MonitorLayoutModal.tsx`
- Modify: `ui/src/i18n/locales/en/workspace.json`
- Modify: `ui/src/i18n/locales/pt-BR/workspace.json`

- [x] **Step 1: buildDefaultLayout sem fake remote**

```typescript
} else {
  remoteVirtual = { id: REMOTE_MONITOR_ID, x: localBounds.maxR, y: localBounds.minY, width, height };
  remoteList = []; // era 1 monitor fake — removido
}
```

- [x] **Step 2: Banner sync pendente no modal**

```tsx
{!layout.remote_monitors?.length && (
  <Text size="sm" c="yellow">{t('layoutEditor.syncPending')}</Text>
)}
```

- [x] **Step 3: Typecheck frontend**

Run: `cd ui; pnpm tsc --noEmit`
Expected: exit 0

---

### Task 5: Persistência + evento UI

**Files:**
- Modify: `crates/winx-application/src/use_cases/kvm_layout_sync.rs` (handle PeerMonitorsAnnounce)
- Existing: `crates/winx-infra/src/kvm_layout_store_toml.rs`

- [x] **Step 1: Ao receber announce, persistir + publicar evento + atualizar layout ativo**

```rust
store_peer_monitors(&deps.store, &deps.bus, peer_id, monitors.clone()).await;
if active_peer == peer_id {
    active.remote_monitors = monitors;
    active.infer_edges_from_geometry();
}
```

- [x] **Step 2: Teste manual 2 PCs** *(pendente execução pelo usuário — ver instruções abaixo)*

1. Rebuild **ambos** PCs (`cargo tauri build` ou `cargo tauri dev`)
2. Conectar Single Connection
3. Log deve mostrar: `monitores do peer recebidos via sync count=2` (PC principal com 2 monitores)
4. Abrir editor no PC remoto → 1 local + 2 remotos (não 1+1)
5. Cruzar borda superior → foco remoto permanece >2s sem bounce

---

### Task 6: save_kvm_layout broadcast

**Files:**
- Modify: `crates/winx-application/src/use_cases/input_control.rs`

- [x] **Step 1: broadcast via clipboard deps (não transport direto)**

```rust
if let (Some(deps), Some(clipboard)) = (
    self.layout_sync_deps.lock().await.clone(),
    self.clipboard.lock().await.clone(),
) {
    kvm_layout_sync::broadcast_kvm_layout(deps.as_ref(), clipboard.as_ref(), peer_id, local, &layout).await;
}
```

Run: `cargo build -p winx-application`
Expected: compila

---

## Self-Review

**Spec coverage:**
| Requisito | Task |
|-----------|------|
| Sync monitores Single Connection | Task 1, 2, 5 |
| Editor mostra contagem correta | Task 4, 5 |
| Mouse crossing sem bounce | Task 3 |
| Salvar layout propaga ao peer | Task 6 |

**Placeholder scan:** Nenhum TBD/TODO genérico.

**Type consistency:** `KvmLayoutSyncDeps`, `announce_layout_sync`, `broadcast_kvm_layout` assinaturas alinhadas.

**Gaps restantes:** Teste manual W4.13-style nos 2 PCs; garantir PROTOCOL_VERSION=6 em ambos (mismatch v5 → decode falha silenciosa no warn log).

---

## Verificação final

```powershell
cargo test -p winx-domain input_control::edge
cargo test --workspace
cargo build -p winx-kvm
cd ui; pnpm tsc --noEmit; pnpm lint
```

**Log de sucesso esperado após connect:**
```
INFO ... monitores do peer recebidos via sync peer_id=... count=2
INFO ... borda atingida — trocando foco para remoto ...
(sem "voltando para foco local" nos próximos 450ms sem movimento intencional)
```
