# Sprint W3 — Sync, Mirror, Órfão Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar propagação de mudanças (LWW), comportamento de mirrors órfãos e UI de membros/detalhe para a feature Workspaces do Winx-KVM.

**Architecture:** Owner edita workspace via `WorkspacePatch`, incrementa `version` monotonicamente e propaga `WorkspaceSyncPayload` assinado (Ed25519) para todos os membros via UDP autenticado (mesmo transport do invite). Mirrors aplicam LWW (`incoming.version > local.version`). Delete do owner envia `WorkspaceDeletePayload`; receptores mantêm o mirror mas marcam `is_orphan = true`. Loop watcher por device emite `member-presence-changed` quando `owner_last_seen > 30s`.

**Tech Stack:** Rust (winx-domain/application/infra/protocol/kvm), Tauri 2.11, ed25519-dalek, tokio (broadcast/RwLock/sleep), React 19.2, Mantine 9.0, Zustand, react-i18next.

**Pre-requisitos verificados:**
- Domain já tem `Workspace::rename/add_member/remove_member/update_layout/apply_sync/to_snapshot/mark_orphan` ([workspace.rs:93-187](../../../crates/winx-domain/src/workspace/workspace.rs#L93)) — operações são puras, sem rede.
- Eventos `WorkspaceMarkedOrphan`, `WorkspaceSyncApplied`, `WorkspaceSyncDiscarded` já existem em [events.rs:100-119](../../../crates/winx-domain/src/workspace/events.rs#L100).
- `OwnershipMode::mark_orphan()` e `touch_owner_seen()` em [ownership.rs:40-60](../../../crates/winx-domain/src/workspace/ownership.rs#L40).
- Transport UDP autenticado por Ed25519 já está pronto via `UdpWorkspaceInviteTransport` (Sprint W2).
- `WorkspaceService` recebe `secret_store`, pode assinar payloads.

---

## File structure

| Camada | Arquivos | Responsabilidade |
|---|---|---|
| `winx-protocol` | `src/workspace.rs` | Adicionar `WorkspaceSyncPayload`, `WorkspaceDeletePayload` + variantes no enum |
| `winx-domain` | `src/workspace/events.rs` | Adicionar `MemberPresenceChanged` event |
| `winx-domain` | `src/shared/error.rs` | Adicionar código `WorkspaceNotOwner` |
| `winx-application` | `src/use_cases/workspace.rs` | `WorkspacePatch` enum, `update_workspace`, refactor `delete_workspace`, `forget_workspace`, `handle_workspace_sync`, `handle_workspace_delete`, `run_presence_watcher` |
| `winx-application` | `src/ports/workspace_transport.rs` | (sem mudança — port atual cobre Sync/Delete via enum) |
| `winx-kvm` | `src/commands/workspace.rs` | Commands `update_workspace_name`, `update_workspace_layout`, `add_workspace_member`, `remove_workspace_member`, `forget_workspace`, `leave_workspace` |
| `winx-kvm` | `src/events/mod.rs` | Mapping de `WorkspaceSyncApplied`, `WorkspaceMarkedOrphan`, `MemberPresenceChanged` |
| `winx-kvm` | `src/lib.rs` | Spawn das tasks `run_invite_listener` (já existe) e `run_presence_watcher` |
| `ui/` | `src/store/workspaceStore.ts` | Adicionar `presenceByDevice: Map<DeviceId, "online" \| "offline">` |
| `ui/` | `src/components/workspace/WorkspaceCard.tsx` | Badges Mirror (com username real), Órfão, Disponível/Indisponível |
| `ui/` | `src/components/workspace/WorkspaceMembersPanel.tsx` (novo) | Lista membros, status online/offline, "Convidar", "Remover" |
| `ui/` | `src/components/workspace/WorkspaceDetailDrawer.tsx` (novo) | Drawer com nome editável + members panel + leave + delete |
| `ui/` | `src/components/workspace/ForgetOrphanButton.tsx` (novo) | Botão "Esquecer este workspace" só visível em mirror órfão |
| `ui/` | `src/ipc/commands.ts` | Wrappers tipados dos novos commands |
| `ui/` | `src/ipc/events.ts` | Tipos dos novos eventos (`workspace-marked-orphan`, `workspace-member-presence`) |
| `ui/` | `src/i18n/locales/{en,pt-BR}/workspace.json` | Novas chaves (`detail.*`, `members.*`, `orphan.*`, `presence.*`) |

---

## Task 1 — Adicionar payloads `Sync` e `Delete` no protocol

**Files:**
- Modify: `crates/winx-protocol/src/workspace.rs`
- Modify: `crates/winx-protocol/src/lib.rs` (bump `PROTOCOL_VERSION` se necessário; verificar)

- [ ] **Step 1.1: Adicionar variantes ao enum `WorkspaceInviteMessage`**

Em `crates/winx-protocol/src/workspace.rs`, modificar o enum para incluir Sync e Delete:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkspaceInviteMessage {
    Invite(WorkspaceInvitePayload),
    Response(WorkspaceInviteResponsePayload),
    Cancel(WorkspaceInviteCancelPayload),
    Sync(WorkspaceSyncPayload),
    Delete(WorkspaceDeletePayload),
}
```

- [ ] **Step 1.2: Adicionar `WorkspaceSyncPayload` ao final do arquivo**

```rust
/// Sincronização incremental de um workspace.
///
/// O `sender_pubkey` é validado pela assinatura do datagrama (mesmo modelo
/// do invite). Receptores aplicam LWW: `incoming.version > local.version`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceSyncPayload {
    pub workspace_id: Uuid,
    pub snapshot: WorkspaceSnapshotPayload,
    pub sender_device_id: Uuid,
    pub sender_pubkey: [u8; 32],
}
```

- [ ] **Step 1.3: Adicionar `WorkspaceDeletePayload` ao final do arquivo**

```rust
/// Notificação de deleção de workspace pelo owner.
///
/// Receptores que possuem um mirror desse workspace marcam `is_orphan = true`
/// mas não removem o mirror.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceDeletePayload {
    pub workspace_id: Uuid,
    pub sender_device_id: Uuid,
    pub sender_pubkey: [u8; 32],
}
```

- [ ] **Step 1.4: Atualizar `decode_and_verify_datagram` para extrair pubkey de Sync/Delete**

Em `crates/winx-infra/src/workspace_invite_udp.rs`, atualizar o `match` que extrai `sender_pubkey`:

```rust
let sender_pubkey = match &msg {
    WorkspaceInviteMessage::Invite(p) => p.sender_pubkey,
    WorkspaceInviteMessage::Response(p) => p.responder_pubkey,
    WorkspaceInviteMessage::Sync(p) => p.sender_pubkey,
    WorkspaceInviteMessage::Delete(p) => p.sender_pubkey,
    WorkspaceInviteMessage::Cancel(_) => {
        return Err(anyhow!("Cancel message has no pubkey"));
    }
};
```

- [ ] **Step 1.5: Adicionar testes de roundtrip para Sync/Delete em `workspace_invite_udp.rs`**

No módulo `#[cfg(test)]`, adicionar dois testes seguindo o padrão de `roundtrip_signed_response`:

```rust
#[test]
fn roundtrip_signed_sync() {
    let signing_key = make_test_signing_key();
    let pubkey = signing_key.verifying_key().to_bytes();
    let snapshot = WorkspaceSnapshotPayload {
        id: Uuid::new_v4(),
        name: "WS".to_string(),
        owner_device_id: Uuid::new_v4(),
        owner_username: "Owner".to_string(),
        version: 5,
        members: vec![],
    };
    let msg = WorkspaceInviteMessage::Sync(winx_protocol::workspace::WorkspaceSyncPayload {
        workspace_id: snapshot.id,
        snapshot,
        sender_device_id: Uuid::new_v4(),
        sender_pubkey: pubkey,
    });
    let datagram =
        UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
    let (decoded, decoded_pubkey) =
        UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();
    assert_eq!(decoded_pubkey, pubkey);
    assert!(matches!(decoded, WorkspaceInviteMessage::Sync(_)));
}

#[test]
fn roundtrip_signed_delete() {
    let signing_key = make_test_signing_key();
    let pubkey = signing_key.verifying_key().to_bytes();
    let msg = WorkspaceInviteMessage::Delete(winx_protocol::workspace::WorkspaceDeletePayload {
        workspace_id: Uuid::new_v4(),
        sender_device_id: Uuid::new_v4(),
        sender_pubkey: pubkey,
    });
    let datagram =
        UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
    let (decoded, _) =
        UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();
    assert!(matches!(decoded, WorkspaceInviteMessage::Delete(_)));
}
```

- [ ] **Step 1.6: Rodar os testes**

```powershell
cargo test -p winx-infra workspace_invite_udp::tests
```

Expected: 6 testes passam (4 existentes + 2 novos).

- [ ] **Step 1.7: Commit**

```powershell
git add crates/winx-protocol/src/workspace.rs crates/winx-infra/src/workspace_invite_udp.rs
git commit -m "feat(workspace): add Sync/Delete payloads to protocol"
```

---

## Task 2 — Adicionar erro `WorkspaceNotOwner` no domain

**Files:**
- Modify: `crates/winx-domain/src/shared/error.rs`

- [ ] **Step 2.1: Adicionar variante ao enum `DomainErrorCode`**

Após `WorkspaceMirrorImmutable`:

```rust
WorkspaceMirrorImmutable,
WorkspaceNotOwner,
```

- [ ] **Step 2.2: Atualizar o `as_str()` correspondente**

Adicionar arm:

```rust
Self::WorkspaceMirrorImmutable => "workspace.mirror_immutable",
Self::WorkspaceNotOwner => "workspace.not_owner",
```

- [ ] **Step 2.3: Rodar testes do domain**

```powershell
cargo test -p winx-domain
```

Expected: todos passam.

- [ ] **Step 2.4: Commit**

```powershell
git add crates/winx-domain/src/shared/error.rs
git commit -m "feat(workspace): add WorkspaceNotOwner error code"
```

---

## Task 3 — Adicionar evento `MemberPresenceChanged`

**Files:**
- Modify: `crates/winx-domain/src/workspace/events.rs`
- Modify: `crates/winx-domain/src/shared/events.rs` (registrar no enum `DomainEvent`)

- [ ] **Step 3.1: Adicionar struct ao final de `workspace/events.rs`**

```rust
/// Estado de presença de um membro do workspace mudou (online/offline).
///
/// Emitido pelo `presence_watcher` quando `owner_last_seen` cruza o threshold
/// de 30s sem heartbeat/sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberPresenceChanged {
    pub workspace_id: WorkspaceId,
    pub device_id: DeviceId,
    pub is_online: bool,
}
```

- [ ] **Step 3.2: Adicionar variante ao enum `DomainEvent`**

Em `crates/winx-domain/src/shared/events.rs`, encontrar o enum `DomainEvent` e adicionar:

```rust
WorkspaceMemberPresenceChanged(crate::workspace::events::MemberPresenceChanged),
```

(seguir ordem alfabética/agrupamento de variantes workspace existente)

- [ ] **Step 3.3: Compilar para validar**

```powershell
cargo check -p winx-domain
```

Expected: sucesso.

- [ ] **Step 3.4: Commit**

```powershell
git add crates/winx-domain/src/workspace/events.rs crates/winx-domain/src/shared/events.rs
git commit -m "feat(workspace): add MemberPresenceChanged domain event"
```

---

## Task 4 — Definir `WorkspacePatch` enum no application

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 4.1: Adicionar enum no topo do arquivo (depois dos `use`)**

Logo após `struct PendingInviteData`:

```rust
/// Mutação aplicável a um Workspace via use case `update_workspace`.
///
/// Cada variante mapeia 1-para-1 com uma operação no aggregate `Workspace`.
#[derive(Debug, Clone)]
pub enum WorkspacePatch {
    Rename {
        new_name: String,
    },
    AddMember {
        device_id: DeviceId,
        public_key: PublicKey,
        username: String,
    },
    RemoveMember {
        device_id: DeviceId,
    },
    UpdateLayout {
        device_id: DeviceId,
        layout: winx_domain::input_control::layout::MonitorLayout,
    },
}
```

- [ ] **Step 4.2: Compilar (a unused warning é OK por enquanto)**

```powershell
cargo check -p winx-application
```

Expected: sucesso, com warning de "unused enum".

- [ ] **Step 4.3: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): introduce WorkspacePatch enum"
```

---

## Task 5 — Implementar `update_workspace` use case

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 5.1: Adicionar helper privado `apply_patch_local`**

Antes do bloco `#[cfg(test)]`:

```rust
fn apply_patch_local(ws: &mut Workspace, patch: WorkspacePatch) -> Result<(), DomainError> {
    let map_err = |e: String| {
        if e == "workspace.mirror_immutable" {
            DomainError::new(DomainErrorCode::WorkspaceMirrorImmutable, e)
        } else {
            DomainError::new(DomainErrorCode::InternalError, e)
        }
    };
    match patch {
        WorkspacePatch::Rename { new_name } => ws.rename(new_name).map_err(map_err),
        WorkspacePatch::AddMember { device_id, public_key, username } => {
            let member = WorkspaceMember::new(device_id, public_key, username);
            ws.add_member(member).map_err(map_err)
        }
        WorkspacePatch::RemoveMember { device_id } => ws.remove_member(device_id).map_err(map_err),
        WorkspacePatch::UpdateLayout { device_id, layout } => {
            ws.update_layout(device_id, layout).map_err(map_err)
        }
    }
}
```

- [ ] **Step 5.2: Adicionar método `update_workspace` em `impl WorkspaceService`**

Logo após `delete_workspace`:

```rust
/// Aplica uma mutação local e propaga `Sync` para todos os membros.
pub async fn update_workspace(
    &self,
    workspace_id: WorkspaceId,
    patch: WorkspacePatch,
) -> Result<Workspace, DomainError> {
    let mut ws = self
        .store
        .find_by_id(workspace_id)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
        .ok_or_else(|| {
            DomainError::new(DomainErrorCode::InternalError, "workspace not found")
        })?;

    if ws.ownership_mode.is_mirror() {
        return Err(DomainError::new(
            DomainErrorCode::WorkspaceMirrorImmutable,
            "mirrors cannot be edited locally",
        ));
    }

    apply_patch_local(&mut ws, patch)?;

    self.store
        .save(&ws)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

    self.bus
        .publish(DomainEvent::WorkspaceSyncApplied(
            winx_domain::workspace::events::WorkspaceSyncApplied {
                workspace_id,
                new_version: ws.version.as_u64(),
            },
        ));

    self.broadcast_sync(&ws).await;

    Ok(ws)
}

/// Envia `WorkspaceSyncPayload` assinado para todos os membros (exceto self).
async fn broadcast_sync(&self, ws: &Workspace) {
    let signing_key = match self.load_signing_key().await {
        Ok(k) => k,
        Err(e) => {
            warn!(workspace_id = %ws.id, ?e, "failed to load signing key for sync broadcast");
            return;
        }
    };
    let local_device = match self.identity_store.load_device().await {
        Ok(Some(d)) => d,
        _ => return,
    };
    let sender_pubkey = *local_device.public_key.as_bytes();

    let snapshot = build_snapshot_payload(ws);

    for member in &ws.members {
        if member.device_id == local_device.id {
            continue;
        }
        let payload = winx_protocol::workspace::WorkspaceSyncPayload {
            workspace_id: ws.id.as_uuid(),
            snapshot: snapshot.clone(),
            sender_device_id: self.local_device_id,
            sender_pubkey,
        };
        let msg = WorkspaceInviteMessage::Sync(payload);
        let target_device_id = member.device_id;
        let svc = self.clone();
        let signing_key_clone = signing_key.clone();
        tokio::spawn(async move {
            match svc.discovery_query.resolve_address(target_device_id).await {
                Ok(Some(mut addr)) => {
                    addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                    if let Err(e) = svc.transport.send_to(addr, &msg, &signing_key_clone).await {
                        warn!(?e, "failed to send workspace sync");
                    }
                }
                Ok(None) => debug!(%target_device_id, "member offline, sync skipped"),
                Err(e) => warn!(?e, "failed to resolve member addr"),
            }
        });
    }
}
```

- [ ] **Step 5.3: Extrair helper `build_snapshot_payload` reutilizável**

Após `apply_patch_local`:

```rust
fn build_snapshot_payload(ws: &Workspace) -> winx_protocol::workspace::WorkspaceSnapshotPayload {
    let members_snapshot: Vec<MemberSnapshotPayload> = ws
        .members
        .iter()
        .map(|m| MemberSnapshotPayload {
            device_id: m.device_id.as_uuid(),
            public_key: *m.public_key.as_bytes(),
            username: m.username_cache.clone(),
            joined_at_rfc3339: m
                .joined_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        })
        .collect();

    let owner_username = ws
        .members
        .iter()
        .find(|m| m.device_id == ws.owner_device_id)
        .map(|m| m.username_cache.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    winx_protocol::workspace::WorkspaceSnapshotPayload {
        id: ws.id.as_uuid(),
        name: ws.name.clone(),
        owner_device_id: ws.owner_device_id.as_uuid(),
        owner_username,
        version: ws.version.as_u64(),
        members: members_snapshot,
    }
}
```

- [ ] **Step 5.4: Refatorar `invite_to_workspace` para usar `build_snapshot_payload`**

Encontrar o bloco em `invite_to_workspace` que constrói o snapshot inline (linhas ~211-244) e substituir por:

```rust
let snapshot = build_snapshot_payload(&ws);
```

(Remove ~30 linhas duplicadas.)

- [ ] **Step 5.5: Adicionar `SigningKey` derive `Clone`**

`SigningKey` de `ed25519_dalek` já é `Clone`. Confirmar compilação:

```powershell
cargo check -p winx-application
```

Expected: sucesso.

- [ ] **Step 5.6: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): implement update_workspace use case with sync broadcast"
```

---

## Task 6 — Adicionar testes E2E para `update_workspace`

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs` (módulo `tests`)

- [ ] **Step 6.1: Adicionar teste de rename**

No módulo `tests`, após `accept_invite_does_tofu_and_creates_mirror`:

```rust
#[tokio::test]
async fn update_workspace_rename_increments_version_and_persists() {
    let (svc, store, transport, _) = make_service();
    let ws = svc.create_workspace("Old Name".to_string(), vec![]).await.unwrap();
    let v0 = ws.version.as_u64();

    let updated = svc
        .update_workspace(ws.id, WorkspacePatch::Rename { new_name: "New Name".to_string() })
        .await
        .unwrap();

    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.version.as_u64(), v0 + 1);

    let loaded = store.load_all().await.unwrap();
    assert_eq!(loaded[0].name, "New Name");

    // Solo workspace (apenas owner) — não há membros remotos pra enviar Sync
    assert!(transport.sent.lock().unwrap().is_empty());
}
```

- [ ] **Step 6.2: Adicionar teste de update em mirror retorna erro**

```rust
#[tokio::test]
async fn update_mirror_returns_mirror_immutable_error() {
    let (svc, store, _, _) = make_service();
    // Setup: criar um mirror diretamente no store
    let owner_member = WorkspaceMember::new(
        DeviceId::from_uuid(Uuid::new_v4()),
        PublicKey::new([1u8; 32]),
        "Other".to_string(),
    );
    let snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Mirror".to_string(),
        owner_device_id: owner_member.device_id,
        version: winx_domain::workspace::WorkspaceVersion::initial(),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(snapshot, "Other");
    let mirror_id = mirror.id;
    store.save(&mirror).await.unwrap();

    let err = svc
        .update_workspace(mirror_id, WorkspacePatch::Rename { new_name: "x".into() })
        .await
        .unwrap_err();

    assert_eq!(err.code, DomainErrorCode::WorkspaceMirrorImmutable);
}
```

- [ ] **Step 6.3: Adicionar teste de broadcast para membro remoto**

```rust
#[tokio::test]
async fn update_workspace_broadcasts_sync_to_remote_members() {
    let remote_uuid = Uuid::new_v4();
    let remote_device = DeviceId::from_uuid(remote_uuid);
    let peer_addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();

    let (svc, _, transport, _) =
        make_service_with_peer_addr(Some(remote_device), Some(peer_addr));

    let mut ws = svc.create_workspace("WS".to_string(), vec![]).await.unwrap();
    // Manually inject a remote member into the persisted workspace
    let remote_member = WorkspaceMember::new(
        remote_device,
        PublicKey::new([8u8; 32]),
        "Remote".to_string(),
    );
    ws.add_member(remote_member).unwrap();
    // Save the modified workspace; this puts the remote member in the persisted snapshot
    let store = svc.store.clone();
    store.save(&ws).await.unwrap();

    svc.update_workspace(ws.id, WorkspacePatch::Rename { new_name: "Renamed".into() })
        .await
        .unwrap();

    // Allow tokio::spawn to fire
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        if !transport.sent.lock().unwrap().is_empty() {
            break;
        }
    }

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    let (_, msg) = &sent[0];
    assert!(matches!(msg, WorkspaceInviteMessage::Sync(_)));
}
```

- [ ] **Step 6.4: Rodar testes**

```powershell
cargo test -p winx-application use_cases::workspace::tests
```

Expected: todos passam (11 testes).

- [ ] **Step 6.5: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "test(workspace): cover update_workspace and broadcast"
```

---

## Task 7 — Refatorar `delete_workspace` para notificar membros

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 7.1: Substituir o `delete_workspace` atual**

Localizar o método e substituir por:

```rust
pub async fn delete_workspace(&self, id: WorkspaceId) -> Result<(), DomainError> {
    let ws = self
        .store
        .find_by_id(id)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
        .ok_or_else(|| {
            DomainError::new(DomainErrorCode::InternalError, "workspace not found")
        })?;

    if ws.ownership_mode.is_mirror() {
        return Err(DomainError::new(
            DomainErrorCode::WorkspaceNotOwner,
            "use forget_workspace to remove a mirror locally",
        ));
    }

    // Notify all members BEFORE removing locally (best-effort)
    self.broadcast_delete(&ws).await;

    self.store
        .delete(id)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

    self.bus
        .publish(DomainEvent::WorkspaceDeleted(
            winx_domain::workspace::events::WorkspaceDeleted { workspace_id: id },
        ));

    info!(%id, "workspace deleted");
    Ok(())
}

async fn broadcast_delete(&self, ws: &Workspace) {
    let signing_key = match self.load_signing_key().await {
        Ok(k) => k,
        Err(e) => {
            warn!(?e, "failed to load signing key for delete broadcast");
            return;
        }
    };
    let local_device = match self.identity_store.load_device().await {
        Ok(Some(d)) => d,
        _ => return,
    };
    let sender_pubkey = *local_device.public_key.as_bytes();
    let workspace_id = ws.id.as_uuid();

    for member in &ws.members {
        if member.device_id == local_device.id {
            continue;
        }
        let payload = winx_protocol::workspace::WorkspaceDeletePayload {
            workspace_id,
            sender_device_id: self.local_device_id,
            sender_pubkey,
        };
        let msg = WorkspaceInviteMessage::Delete(payload);
        let target_device_id = member.device_id;
        let svc = self.clone();
        let signing_key_clone = signing_key.clone();
        tokio::spawn(async move {
            match svc.discovery_query.resolve_address(target_device_id).await {
                Ok(Some(mut addr)) => {
                    addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                    if let Err(e) = svc.transport.send_to(addr, &msg, &signing_key_clone).await {
                        warn!(?e, "failed to send workspace delete");
                    }
                }
                Ok(None) => debug!(%target_device_id, "member offline, delete notice skipped"),
                Err(e) => warn!(?e, "failed to resolve member addr"),
            }
        });
    }
}
```

- [ ] **Step 7.2: Compilar e validar**

```powershell
cargo check -p winx-application
```

Expected: sucesso.

- [ ] **Step 7.3: Atualizar teste E2E existente de delete (se aplicável)**

Procurar testes que chamam `delete_workspace` em mirrors — devem agora esperar `WorkspaceNotOwner`. Caso não exista, adicionar:

```rust
#[tokio::test]
async fn delete_mirror_returns_not_owner_error() {
    let (svc, store, _, _) = make_service();
    let owner_member = WorkspaceMember::new(
        DeviceId::from_uuid(Uuid::new_v4()),
        PublicKey::new([1u8; 32]),
        "Other".to_string(),
    );
    let snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Mirror".to_string(),
        owner_device_id: owner_member.device_id,
        version: winx_domain::workspace::WorkspaceVersion::initial(),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(snapshot, "Other");
    let mirror_id = mirror.id;
    store.save(&mirror).await.unwrap();

    let err = svc.delete_workspace(mirror_id).await.unwrap_err();
    assert_eq!(err.code, DomainErrorCode::WorkspaceNotOwner);
}
```

- [ ] **Step 7.4: Rodar testes**

```powershell
cargo test -p winx-application use_cases::workspace::tests
```

Expected: todos passam.

- [ ] **Step 7.5: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): delete_workspace notifies members before removing"
```

---

## Task 8 — Implementar `forget_workspace` (remoção local de mirror)

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 8.1: Adicionar método `forget_workspace`**

Logo após `delete_workspace`:

```rust
/// Remove um mirror localmente sem notificar o owner. Usado para órfãos
/// ou para "sair" voluntariamente.
pub async fn forget_workspace(&self, id: WorkspaceId) -> Result<(), DomainError> {
    let ws = self
        .store
        .find_by_id(id)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
        .ok_or_else(|| {
            DomainError::new(DomainErrorCode::InternalError, "workspace not found")
        })?;

    if !ws.ownership_mode.is_mirror() {
        return Err(DomainError::new(
            DomainErrorCode::WorkspaceMirrorImmutable,
            "use delete_workspace on originals",
        ));
    }

    self.store
        .delete(id)
        .await
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

    self.bus
        .publish(DomainEvent::WorkspaceDeleted(
            winx_domain::workspace::events::WorkspaceDeleted { workspace_id: id },
        ));

    info!(%id, "mirror forgotten locally");
    Ok(())
}
```

- [ ] **Step 8.2: Adicionar teste E2E**

```rust
#[tokio::test]
async fn forget_workspace_removes_mirror_locally() {
    let (svc, store, _, _) = make_service();
    let owner_member = WorkspaceMember::new(
        DeviceId::from_uuid(Uuid::new_v4()),
        PublicKey::new([1u8; 32]),
        "Other".to_string(),
    );
    let snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Mirror".to_string(),
        owner_device_id: owner_member.device_id,
        version: winx_domain::workspace::WorkspaceVersion::initial(),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(snapshot, "Other");
    let mirror_id = mirror.id;
    store.save(&mirror).await.unwrap();

    svc.forget_workspace(mirror_id).await.unwrap();
    assert!(store.load_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn forget_workspace_on_original_returns_error() {
    let (svc, _, _, _) = make_service();
    let ws = svc.create_workspace("Mine".to_string(), vec![]).await.unwrap();
    let err = svc.forget_workspace(ws.id).await.unwrap_err();
    assert_eq!(err.code, DomainErrorCode::WorkspaceMirrorImmutable);
}
```

- [ ] **Step 8.3: Rodar testes**

```powershell
cargo test -p winx-application use_cases::workspace::tests
```

Expected: todos passam.

- [ ] **Step 8.4: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): forget_workspace removes mirror without notifying owner"
```

---

## Task 9 — Handler `handle_workspace_sync` no listener

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 9.1: Adicionar variante no `match` do `run_invite_listener`**

Localizar `pub async fn run_invite_listener` e atualizar o `match` para incluir Sync/Delete:

```rust
match &decoded.message {
    WorkspaceInviteMessage::Invite(payload) => {
        self.handle_incoming_invite(payload).await;
    }
    WorkspaceInviteMessage::Response(payload) => {
        self.handle_invite_response(payload).await;
    }
    WorkspaceInviteMessage::Sync(payload) => {
        self.handle_workspace_sync(payload).await;
    }
    WorkspaceInviteMessage::Delete(payload) => {
        self.handle_workspace_delete(payload).await;
    }
    WorkspaceInviteMessage::Cancel(_) => {
        debug!("received invite cancellation");
    }
}
```

- [ ] **Step 9.2: Implementar `handle_workspace_sync`**

Adicionar método após `handle_invite_response`:

```rust
async fn handle_workspace_sync(
    &self,
    payload: &winx_protocol::workspace::WorkspaceSyncPayload,
) {
    let workspace_id = WorkspaceId::from_uuid(payload.workspace_id);

    let mut ws = match self.store.find_by_id(workspace_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            debug!(%workspace_id, "received sync for unknown workspace, ignoring");
            return;
        }
        Err(e) => {
            warn!(?e, %workspace_id, "failed to load workspace for sync");
            return;
        }
    };

    // Convert protocol snapshot → domain snapshot
    let domain_members: Vec<WorkspaceMember> = payload
        .snapshot
        .members
        .iter()
        .map(|m| {
            WorkspaceMember::new(
                DeviceId::from_uuid(m.device_id),
                PublicKey::new(m.public_key),
                m.username.clone(),
            )
        })
        .collect();

    let domain_snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: workspace_id,
        name: payload.snapshot.name.clone(),
        owner_device_id: DeviceId::from_uuid(payload.snapshot.owner_device_id),
        version: winx_domain::workspace::WorkspaceVersion::from_u64(payload.snapshot.version),
        ownership_mode: ws.ownership_mode.clone(),
        members: domain_members,
        layout: ws.layout.clone(), // layout não vem no MVP de sync; preservar local
    };

    let local_version = ws.version.as_u64();
    let outcome = ws.apply_sync(domain_snapshot);

    match outcome {
        winx_domain::workspace::SyncOutcome::Applied => {
            // Mirror: refresh owner_last_seen
            if let winx_domain::workspace::OwnershipMode::Mirror { .. } = &mut ws.ownership_mode {
                let _ = ws.ownership_mode.touch_owner_seen();
            }
            if let Err(e) = self.store.save(&ws).await {
                warn!(?e, %workspace_id, "failed to persist synced workspace");
                return;
            }
            self.bus.publish(DomainEvent::WorkspaceSyncApplied(
                winx_domain::workspace::events::WorkspaceSyncApplied {
                    workspace_id,
                    new_version: ws.version.as_u64(),
                },
            ));
            info!(%workspace_id, new_version = ws.version.as_u64(), "sync applied (LWW)");
        }
        winx_domain::workspace::SyncOutcome::Discarded { incoming_version, .. } => {
            self.bus.publish(DomainEvent::WorkspaceSyncDiscarded(
                winx_domain::workspace::events::WorkspaceSyncDiscarded {
                    workspace_id,
                    local_version,
                    incoming_version,
                },
            ));
            debug!(%workspace_id, local_version, incoming_version, "sync discarded (LWW)");
        }
    }
}
```

- [ ] **Step 9.3: Garantir que `WorkspaceVersion` tem método `from_u64`**

Verificar em `crates/winx-domain/src/workspace/version.rs`. Se não existir, adicionar:

```rust
impl WorkspaceVersion {
    pub fn from_u64(v: u64) -> Self {
        Self(v)
    }
}
```

- [ ] **Step 9.4: Compilar**

```powershell
cargo check -p winx-application
```

Expected: sucesso.

- [ ] **Step 9.5: Adicionar teste E2E split-brain LWW**

```rust
#[tokio::test]
async fn handle_workspace_sync_applies_when_incoming_version_higher() {
    let (svc, store, _, _) = make_service();
    // Create a mirror locally at version 1
    let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
    let owner_member = WorkspaceMember::new(
        owner_device_id,
        PublicKey::new([1u8; 32]),
        "Owner".to_string(),
    );
    let initial_snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Original".to_string(),
        owner_device_id,
        version: winx_domain::workspace::WorkspaceVersion::initial(),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member.clone()],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(initial_snapshot.clone(), "Owner");
    let workspace_id = mirror.id;
    store.save(&mirror).await.unwrap();

    // Build sync payload with version=5 and renamed
    let sync_payload = winx_protocol::workspace::WorkspaceSyncPayload {
        workspace_id: workspace_id.as_uuid(),
        snapshot: winx_protocol::workspace::WorkspaceSnapshotPayload {
            id: workspace_id.as_uuid(),
            name: "Renamed".to_string(),
            owner_device_id: owner_device_id.as_uuid(),
            owner_username: "Owner".to_string(),
            version: 5,
            members: vec![winx_protocol::workspace::MemberSnapshotPayload {
                device_id: owner_device_id.as_uuid(),
                public_key: [1u8; 32],
                username: "Owner".to_string(),
                joined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
            }],
        },
        sender_device_id: owner_device_id.as_uuid(),
        sender_pubkey: [1u8; 32],
    };

    svc.handle_workspace_sync(&sync_payload).await;

    let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
    assert_eq!(loaded.name, "Renamed");
    assert_eq!(loaded.version.as_u64(), 5);
}

#[tokio::test]
async fn handle_workspace_sync_discards_when_incoming_version_lower() {
    let (svc, store, _, _) = make_service();
    let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
    let owner_member = WorkspaceMember::new(
        owner_device_id,
        PublicKey::new([1u8; 32]),
        "Owner".to_string(),
    );
    let snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Local".to_string(),
        owner_device_id,
        version: winx_domain::workspace::WorkspaceVersion::from_u64(10),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(snapshot, "Owner");
    let workspace_id = mirror.id;
    store.save(&mirror).await.unwrap();

    let sync_payload = winx_protocol::workspace::WorkspaceSyncPayload {
        workspace_id: workspace_id.as_uuid(),
        snapshot: winx_protocol::workspace::WorkspaceSnapshotPayload {
            id: workspace_id.as_uuid(),
            name: "Stale".to_string(),
            owner_device_id: owner_device_id.as_uuid(),
            owner_username: "Owner".to_string(),
            version: 3,
            members: vec![],
        },
        sender_device_id: owner_device_id.as_uuid(),
        sender_pubkey: [1u8; 32],
    };

    svc.handle_workspace_sync(&sync_payload).await;
    let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
    assert_eq!(loaded.name, "Local"); // unchanged
    assert_eq!(loaded.version.as_u64(), 10);
}
```

- [ ] **Step 9.6: Rodar testes**

```powershell
cargo test -p winx-application use_cases::workspace::tests
```

Expected: todos passam.

- [ ] **Step 9.7: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs crates/winx-domain/src/workspace/version.rs
git commit -m "feat(workspace): handle incoming sync with LWW resolution"
```

---

## Task 10 — Handler `handle_workspace_delete`

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 10.1: Implementar handler**

Após `handle_workspace_sync`:

```rust
async fn handle_workspace_delete(
    &self,
    payload: &winx_protocol::workspace::WorkspaceDeletePayload,
) {
    let workspace_id = WorkspaceId::from_uuid(payload.workspace_id);

    let mut ws = match self.store.find_by_id(workspace_id).await {
        Ok(Some(w)) => w,
        Ok(None) => return,
        Err(e) => {
            warn!(?e, %workspace_id, "failed to load workspace for delete notice");
            return;
        }
    };

    // Only mirrors should be marked orphan; an Original receiving delete is suspicious — ignore.
    if !ws.ownership_mode.is_mirror() {
        warn!(%workspace_id, "received Delete for an Original workspace, ignoring");
        return;
    }

    if let Err(e) = ws.mark_orphan() {
        warn!(?e, %workspace_id, "failed to mark orphan");
        return;
    }

    if let Err(e) = self.store.save(&ws).await {
        warn!(?e, %workspace_id, "failed to persist orphan flag");
        return;
    }

    self.bus.publish(DomainEvent::WorkspaceMarkedOrphan(
        winx_domain::workspace::events::WorkspaceMarkedOrphan { workspace_id },
    ));

    info!(%workspace_id, "mirror marked as orphan after owner delete");
}
```

- [ ] **Step 10.2: Adicionar teste E2E**

```rust
#[tokio::test]
async fn handle_workspace_delete_marks_mirror_orphan() {
    let (svc, store, _, _) = make_service();
    let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
    let owner_member = WorkspaceMember::new(
        owner_device_id,
        PublicKey::new([1u8; 32]),
        "Owner".to_string(),
    );
    let snapshot = winx_domain::workspace::WorkspaceSnapshot {
        id: WorkspaceId::new(),
        name: "Mirror".to_string(),
        owner_device_id,
        version: winx_domain::workspace::WorkspaceVersion::initial(),
        ownership_mode: winx_domain::workspace::OwnershipMode::Original,
        members: vec![owner_member],
        layout: winx_domain::workspace::WorkspaceLayout::empty(),
    };
    let mirror = Workspace::create_mirror(snapshot, "Owner");
    let workspace_id = mirror.id;
    store.save(&mirror).await.unwrap();

    let delete_payload = winx_protocol::workspace::WorkspaceDeletePayload {
        workspace_id: workspace_id.as_uuid(),
        sender_device_id: owner_device_id.as_uuid(),
        sender_pubkey: [1u8; 32],
    };

    svc.handle_workspace_delete(&delete_payload).await;

    let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
    match loaded.ownership_mode {
        winx_domain::workspace::OwnershipMode::Mirror { is_orphan, .. } => {
            assert!(is_orphan);
        }
        _ => panic!("expected mirror"),
    }
}
```

- [ ] **Step 10.3: Rodar testes**

```powershell
cargo test -p winx-application use_cases::workspace::tests
```

Expected: todos passam.

- [ ] **Step 10.4: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): handle owner delete by marking mirror as orphan"
```

---

## Task 11 — Presence watcher (30s heartbeat threshold)

**Files:**
- Modify: `crates/winx-application/src/use_cases/workspace.rs`

- [ ] **Step 11.1: Adicionar campo de presence state ao `WorkspaceService`**

Localizar o struct e adicionar:

```rust
pub struct WorkspaceService {
    // ... existing fields
    member_online_state: Arc<Mutex<HashMap<(WorkspaceId, DeviceId), bool>>>,
}
```

E no construtor `new(...)`:

```rust
member_online_state: Arc::new(Mutex::new(HashMap::new())),
```

- [ ] **Step 11.2: Implementar `run_presence_watcher`**

Após `run_expiration_loop`:

```rust
/// Loop que verifica presença de membros baseado em `owner_last_seen` dos mirrors.
///
/// Para cada mirror local com `owner_last_seen` mais antigo que 30s, emite
/// `MemberPresenceChanged { is_online: false }`. Quando o `last_seen` é atualizado
/// (via sync), emite `is_online: true`.
pub async fn run_presence_watcher(&self) {
    const CHECK_INTERVAL_SECS: u64 = 5;
    const OFFLINE_THRESHOLD_SECS: i64 = 30;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;

        let workspaces = match self.store.load_all().await {
            Ok(ws) => ws,
            Err(e) => {
                warn!(?e, "presence_watcher: failed to load workspaces");
                continue;
            }
        };

        let now = time::OffsetDateTime::now_utc();
        let mut state = self.member_online_state.lock().await;

        for ws in &workspaces {
            if let winx_domain::workspace::OwnershipMode::Mirror { owner_last_seen, .. } =
                &ws.ownership_mode
            {
                let age = (now - *owner_last_seen).whole_seconds();
                let is_online = age < OFFLINE_THRESHOLD_SECS;
                let key = (ws.id, ws.owner_device_id);
                let prev = state.get(&key).copied();
                if prev != Some(is_online) {
                    state.insert(key, is_online);
                    self.bus.publish(DomainEvent::WorkspaceMemberPresenceChanged(
                        winx_domain::workspace::events::MemberPresenceChanged {
                            workspace_id: ws.id,
                            device_id: ws.owner_device_id,
                            is_online,
                        },
                    ));
                }
            }
        }
    }
}
```

- [ ] **Step 11.3: Compilar**

```powershell
cargo check -p winx-application
```

Expected: sucesso.

- [ ] **Step 11.4: Commit**

```powershell
git add crates/winx-application/src/use_cases/workspace.rs
git commit -m "feat(workspace): add presence watcher emitting MemberPresenceChanged"
```

---

## Task 12 — Tauri commands

**Files:**
- Modify: `crates/winx-kvm/src/commands/workspace.rs`
- Modify: `crates/winx-kvm/src/lib.rs` (registrar handlers no `invoke_handler!`)

- [ ] **Step 12.1: Adicionar DTO para input do command de rename/membros**

Antes dos commands:

```rust
#[derive(Debug, serde::Deserialize)]
pub struct AddMemberInput {
    pub workspace_id: String,
    pub device_id: String,
    pub public_key_hex: String,
    pub username: String,
}
```

- [ ] **Step 12.2: Adicionar comando `rename_workspace`**

Após `create_workspace`:

```rust
#[tauri::command]
pub async fn rename_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    new_name: String,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::Rename { new_name },
        )
        .await
        .map_err(map_err)?;
    Ok(ws_to_dto(&ws))
}
```

- [ ] **Step 12.3: Adicionar comando `add_workspace_member`**

```rust
#[tauri::command]
pub async fn add_workspace_member(
    state: State<'_, WorkspaceState>,
    input: AddMemberInput,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&input.workspace_id)?;
    let device_uuid = parse_device_id(&input.device_id)?;
    let pubkey_bytes = hex::decode(&input.public_key_hex)
        .map_err(|e| format!("public_key_hex inválido: {e}"))?;
    let pubkey_arr: [u8; 32] = pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "public_key_hex deve ter 32 bytes".to_string())?;

    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::AddMember {
                device_id: winx_domain::shared::ids::DeviceId::from_uuid(device_uuid),
                public_key: winx_domain::identity::key::PublicKey::new(pubkey_arr),
                username: input.username,
            },
        )
        .await
        .map_err(map_err)?;
    Ok(ws_to_dto(&ws))
}
```

- [ ] **Step 12.4: Adicionar comando `remove_workspace_member`**

```rust
#[tauri::command]
pub async fn remove_workspace_member(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    device_id: String,
) -> Result<WorkspaceDto, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let device_uuid = parse_device_id(&device_id)?;
    let ws = state
        .service
        .update_workspace(
            ws_id,
            winx_application::use_cases::workspace::WorkspacePatch::RemoveMember {
                device_id: winx_domain::shared::ids::DeviceId::from_uuid(device_uuid),
            },
        )
        .await
        .map_err(map_err)?;
    Ok(ws_to_dto(&ws))
}
```

- [ ] **Step 12.5: Adicionar comando `forget_workspace`**

```rust
#[tauri::command]
pub async fn forget_workspace(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<(), String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    state
        .service
        .forget_workspace(ws_id)
        .await
        .map_err(map_err)
}
```

- [ ] **Step 12.6: Registrar handlers em `lib.rs`**

Em `crates/winx-kvm/src/lib.rs`, no `tauri::generate_handler![...]`, adicionar:

```rust
commands::workspace::rename_workspace,
commands::workspace::add_workspace_member,
commands::workspace::remove_workspace_member,
commands::workspace::forget_workspace,
```

- [ ] **Step 12.7: Compilar**

```powershell
cargo check -p winx-kvm
```

Expected: sucesso.

- [ ] **Step 12.8: Commit**

```powershell
git add crates/winx-kvm/src/commands/workspace.rs crates/winx-kvm/src/lib.rs
git commit -m "feat(workspace): expose rename/add_member/remove_member/forget Tauri commands"
```

---

## Task 13 — Mapping de eventos para frontend

**Files:**
- Modify: `crates/winx-kvm/src/events/mod.rs`

- [ ] **Step 13.1: Adicionar campos relevantes ao `FrontendEvent`**

Procurar pelo `pub struct FrontendEvent` e garantir que existem (adicionar se faltar):

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub new_version: Option<u64>,
#[serde(skip_serializing_if = "Option::is_none")]
pub is_online: Option<bool>,
```

- [ ] **Step 13.2: Adicionar arms no `match` `From<&DomainEvent>`**

Antes do `_ => FrontendEvent::empty("unknown")`:

```rust
DomainEvent::WorkspaceMarkedOrphan(e) => FrontendEvent {
    kind: "workspace-marked-orphan",
    workspace_id: Some(e.workspace_id.to_string()),
    ..FrontendEvent::empty("workspace-marked-orphan")
},
DomainEvent::WorkspaceSyncApplied(e) => FrontendEvent {
    kind: "workspace-sync-applied",
    workspace_id: Some(e.workspace_id.to_string()),
    new_version: Some(e.new_version),
    ..FrontendEvent::empty("workspace-sync-applied")
},
DomainEvent::WorkspaceSyncDiscarded(e) => FrontendEvent {
    kind: "workspaces-updated", // não vaza pro UI; só logging
    workspace_id: Some(e.workspace_id.to_string()),
    ..FrontendEvent::empty("workspaces-updated")
},
DomainEvent::WorkspaceMemberPresenceChanged(e) => FrontendEvent {
    kind: "workspace-member-presence",
    workspace_id: Some(e.workspace_id.to_string()),
    peer_id: Some(e.device_id.to_string()),
    is_online: Some(e.is_online),
    ..FrontendEvent::empty("workspace-member-presence")
},
```

- [ ] **Step 13.3: Atualizar arm de `WorkspaceCreated | WorkspaceDeleted` para incluir Sync também**

```rust
DomainEvent::WorkspaceCreated(_)
| DomainEvent::WorkspaceDeleted(_)
| DomainEvent::WorkspaceSyncApplied(_) => FrontendEvent {
    kind: "workspaces-updated",
    ..FrontendEvent::empty("workspaces-updated")
},
```

(Substitui o arm anterior; remove o standalone `WorkspaceSyncApplied` adicionado no Step 13.2 para não duplicar — manter apenas este combinado.)

- [ ] **Step 13.4: Compilar**

```powershell
cargo check -p winx-kvm
```

Expected: sucesso.

- [ ] **Step 13.5: Commit**

```powershell
git add crates/winx-kvm/src/events/mod.rs
git commit -m "feat(workspace): emit sync/orphan/presence events to frontend"
```

---

## Task 14 — Spawn `run_presence_watcher` no setup

**Files:**
- Modify: `crates/winx-kvm/src/lib.rs`

Nota: `run_invite_listener` e `run_expiration_loop` já são spawnados nas linhas 325-335. Esta task **só** adiciona o `run_presence_watcher`.

- [ ] **Step 14.1: Adicionar spawn logo após `workspace_expiration`**

Em [crates/winx-kvm/src/lib.rs:335](../../../crates/winx-kvm/src/lib.rs#L335), após o bloco `workspace_expiration.run_expiration_loop()`:

```rust
    let workspace_presence = Arc::clone(&services.workspace);
    rt.spawn(async move {
        workspace_presence.run_presence_watcher().await;
    });
```

- [ ] **Step 14.2: Validar com `cargo check`**

```powershell
cargo check -p winx-kvm
```

Expected: sucesso.

- [ ] **Step 14.3: Commit**

```powershell
git add crates/winx-kvm/src/lib.rs
git commit -m "feat(workspace): spawn presence_watcher task at startup"
```

---

## Task 15 — IPC wrappers no frontend

**Files:**
- Modify: `ui/src/ipc/commands.ts`

- [ ] **Step 15.1: Adicionar wrappers tipados**

No final do arquivo:

```typescript
export async function renameWorkspace(workspaceId: string, newName: string): Promise<WorkspaceDto> {
  return invoke('rename_workspace', { workspaceId, newName });
}

export async function addWorkspaceMember(input: {
  workspaceId: string;
  deviceId: string;
  publicKeyHex: string;
  username: string;
}): Promise<WorkspaceDto> {
  return invoke('add_workspace_member', { input });
}

export async function removeWorkspaceMember(workspaceId: string, deviceId: string): Promise<WorkspaceDto> {
  return invoke('remove_workspace_member', { workspaceId, deviceId });
}

export async function forgetWorkspace(workspaceId: string): Promise<void> {
  return invoke('forget_workspace', { workspaceId });
}
```

- [ ] **Step 15.2: Rodar typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 15.3: Commit**

```powershell
git add ui/src/ipc/commands.ts
git commit -m "feat(workspace/ui): typed IPC wrappers for new commands"
```

---

## Task 16 — Atualizar `workspaceStore` para tracking de presença e órfão

**Files:**
- Modify: `ui/src/store/workspaceStore.ts`

- [ ] **Step 16.1: Adicionar fields ao state**

Procurar pela definição `interface WorkspaceState` (ou inline `create<...>`). Adicionar:

```typescript
presence: Record<string, boolean>; // key: `${workspaceId}:${deviceId}`
setPresence: (workspaceId: string, deviceId: string, isOnline: boolean) => void;
```

- [ ] **Step 16.2: Inicializar e implementar setter**

No `create((set) => ({...}))`:

```typescript
presence: {},
setPresence: (workspaceId, deviceId, isOnline) =>
  set((state) => ({
    presence: { ...state.presence, [`${workspaceId}:${deviceId}`]: isOnline },
  })),
```

- [ ] **Step 16.3: Adicionar listener pra `workspace-member-presence` no app root**

Em `ui/src/App.tsx` (ou onde o listener global de eventos vive — procurar por `listen('winx://event'`):

```typescript
case 'workspace-member-presence':
  if (payload.workspace_id && payload.peer_id && payload.is_online !== undefined) {
    useWorkspaceStore.getState().setPresence(
      payload.workspace_id,
      payload.peer_id,
      payload.is_online,
    );
  }
  break;
```

- [ ] **Step 16.4: Typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 16.5: Commit**

```powershell
git add ui/src/store/workspaceStore.ts ui/src/App.tsx
git commit -m "feat(workspace/ui): track member presence in store"
```

---

## Task 17 — Atualizar `WorkspaceCard` com badges Mirror/Órfão/Disponível

**Files:**
- Modify: `ui/src/components/workspace/WorkspaceCard.tsx`
- Modify: `ui/src/ipc/commands.ts` (expandir `WorkspaceDto` se necessário)

- [ ] **Step 17.1: Adicionar `owner_username_snapshot` ao `WorkspaceDto`**

Em `commands.ts`:

```typescript
export interface WorkspaceDto {
  id: string;
  name: string;
  owner_device_id: string;
  is_mirror: boolean;
  is_orphan: boolean;
  owner_username: string | null; // present when is_mirror = true
  member_count: number;
  version: number;
}
```

E no backend, em `crates/winx-kvm/src/commands/workspace.rs`, atualizar `WorkspaceDto` Rust struct + `ws_to_dto`:

```rust
#[derive(Debug, Serialize)]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub owner_device_id: String,
    pub is_mirror: bool,
    pub is_orphan: bool,
    pub owner_username: Option<String>,
    pub member_count: usize,
    pub version: u64,
}

fn ws_to_dto(ws: &winx_domain::workspace::Workspace) -> WorkspaceDto {
    let (is_mirror, is_orphan, owner_username) = match &ws.ownership_mode {
        OwnershipMode::Original => (false, false, None),
        OwnershipMode::Mirror { owner_username_snapshot, is_orphan, .. } => {
            (true, *is_orphan, Some(owner_username_snapshot.clone()))
        }
    };

    WorkspaceDto {
        id: ws.id.to_string(),
        name: ws.name.clone(),
        owner_device_id: ws.owner_device_id.to_string(),
        is_mirror,
        is_orphan,
        owner_username,
        member_count: ws.members.len(),
        version: ws.version.as_u64(),
    }
}
```

- [ ] **Step 17.2: Refatorar o `WorkspaceCard.tsx`**

Substituir o corpo do componente. Importar `useWorkspaceStore` para ler `presence`:

```tsx
import { Badge, Button, Card, Group, Stack, Text, Tooltip } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import * as ipc from '../../ipc/commands';
import type { WorkspaceDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto;
  onOpenDetail?: () => void;
}

export default function WorkspaceCard({ workspace, onOpenDetail }: Props) {
  const { t } = useTranslation('workspace');
  const { activeWorkspaceId, presence, setConflict } = useWorkspaceStore();
  const isActive = activeWorkspaceId === workspace.id;

  // "Available" = at least one member online (owner counts for mirrors)
  const ownerKey = `${workspace.id}:${workspace.owner_device_id}`;
  const isAvailable = presence[ownerKey] === true || !workspace.is_mirror;

  const borderColor = workspace.is_mirror ? 'var(--mantine-color-gray-4)' : undefined;

  const handleConnect = async () => {
    try {
      await ipc.connectToWorkspace(workspace.id);
    } catch (err: any) {
      const errorMsg = typeof err === 'string' ? err : err?.message;
      if (errorMsg && errorMsg.includes('workspace.conflict')) {
        try {
          const parsedError = JSON.parse(errorMsg);
          if (parsedError.code === 'workspace.conflict') {
            setConflict({
              activeId: parsedError.active_id,
              targetId: workspace.id,
              activeName: workspace.name,
              targetName: workspace.name,
            });
          }
        } catch {
          console.error('Failed to connect:', err);
        }
      }
    }
  };

  const handleDisconnect = async () => {
    await ipc.disconnectFromWorkspace().catch(console.error);
  };

  const handleDelete = async () => {
    await ipc.deleteWorkspace(workspace.id).catch(console.error);
  };

  const handleForget = async () => {
    await ipc.forgetWorkspace(workspace.id).catch(console.error);
  };

  return (
    <Card withBorder radius="md" p="md" style={{ borderColor }}>
      <Stack gap="xs">
        <Group justify="space-between">
          <Group gap="xs">
            <Text fw={600} onClick={onOpenDetail} style={{ cursor: 'pointer' }}>
              {workspace.name}
            </Text>
            {workspace.is_mirror && workspace.owner_username && (
              <Badge color="gray" variant="light">
                {t('card.mirrorBadge', { username: workspace.owner_username })}
              </Badge>
            )}
            {workspace.is_orphan && (
              <Tooltip label={t('card.orphanTooltip')}>
                <Badge color="orange" variant="filled">{t('card.orphanBadge')}</Badge>
              </Tooltip>
            )}
            <Badge color={isAvailable ? 'green' : 'gray'} variant="dot">
              {isAvailable ? t('card.available') : t('card.unavailable')}
            </Badge>
          </Group>
        </Group>
        <Text size="sm" c="dimmed">
          {t('card.memberCount_other', { count: workspace.member_count })}
        </Text>
        <Group gap="xs">
          {isActive ? (
            <Button size="xs" variant="light" color="red" onClick={handleDisconnect}>
              {t('card.disconnectButton')}
            </Button>
          ) : (
            <Button size="xs" variant="filled" onClick={handleConnect} disabled={!isAvailable}>
              {t('card.connectButton')}
            </Button>
          )}
          {!workspace.is_mirror && (
            <Button size="xs" variant="subtle" color="red" onClick={handleDelete}>
              {t('card.deleteButton')}
            </Button>
          )}
          {workspace.is_mirror && workspace.is_orphan && (
            <Button size="xs" variant="subtle" color="orange" onClick={handleForget}>
              {t('card.forgetButton')}
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  );
}
```

- [ ] **Step 17.3: Typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 17.4: Commit**

```powershell
git add ui/src/components/workspace/WorkspaceCard.tsx ui/src/ipc/commands.ts crates/winx-kvm/src/commands/workspace.rs
git commit -m "feat(workspace/ui): badges Mirror/Orphan/Available on WorkspaceCard"
```

---

## Task 18 — Componente `WorkspaceMembersPanel`

**Files:**
- Create: `ui/src/components/workspace/WorkspaceMembersPanel.tsx`

- [ ] **Step 18.1: Adicionar comando `list_workspace_members` no backend**

Em `crates/winx-kvm/src/commands/workspace.rs`:

```rust
#[derive(Debug, Serialize)]
pub struct WorkspaceMemberDto {
    pub device_id: String,
    pub public_key_hex: String,
    pub username: String,
    pub is_owner: bool,
}

#[tauri::command]
pub async fn list_workspace_members(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
) -> Result<Vec<WorkspaceMemberDto>, String> {
    let ws_id = parse_workspace_id(&workspace_id)?;
    let workspaces = state
        .service
        .list_workspaces()
        .await
        .map_err(|e| format!("falha ao listar workspaces: {e}"))?;
    let ws = workspaces
        .iter()
        .find(|w| w.id == ws_id)
        .ok_or_else(|| "workspace não encontrado".to_string())?;
    Ok(ws
        .members
        .iter()
        .map(|m| WorkspaceMemberDto {
            device_id: m.device_id.to_string(),
            public_key_hex: hex::encode(m.public_key.as_bytes()),
            username: m.username_cache.clone(),
            is_owner: m.device_id == ws.owner_device_id,
        })
        .collect())
}
```

Adicionar ao `invoke_handler!` em `lib.rs`:

```rust
commands::workspace::list_workspace_members,
```

- [ ] **Step 18.2: Adicionar wrapper IPC e tipo**

Em `ui/src/ipc/commands.ts`:

```typescript
export interface WorkspaceMemberDto {
  device_id: string;
  public_key_hex: string;
  username: string;
  is_owner: boolean;
}

export async function listWorkspaceMembers(workspaceId: string): Promise<WorkspaceMemberDto[]> {
  return invoke('list_workspace_members', { workspaceId });
}
```

- [ ] **Step 18.3: Criar `WorkspaceMembersPanel.tsx`**

```tsx
import { useEffect, useState } from 'react';
import { Badge, Button, Group, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import { useWorkspaceStore } from '../../store/workspaceStore';
import type { WorkspaceDto, WorkspaceMemberDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto;
  onInviteClick: () => void;
}

export default function WorkspaceMembersPanel({ workspace, onInviteClick }: Props) {
  const { t } = useTranslation('workspace');
  const presence = useWorkspaceStore((s) => s.presence);
  const [members, setMembers] = useState<WorkspaceMemberDto[]>([]);

  useEffect(() => {
    ipc.listWorkspaceMembers(workspace.id).then(setMembers).catch(console.error);
  }, [workspace.id, workspace.version]);

  const handleRemove = async (deviceId: string) => {
    await ipc.removeWorkspaceMember(workspace.id, deviceId).catch(console.error);
    const refreshed = await ipc.listWorkspaceMembers(workspace.id);
    setMembers(refreshed);
  };

  return (
    <Stack gap="xs">
      <Group justify="space-between">
        <Text fw={600}>{t('members.title')}</Text>
        {!workspace.is_mirror && (
          <Button size="xs" onClick={onInviteClick}>
            {t('members.inviteButton')}
          </Button>
        )}
      </Group>
      {members.map((m) => {
        const isOnline = presence[`${workspace.id}:${m.device_id}`] === true || m.is_owner;
        return (
          <Group key={m.device_id} justify="space-between">
            <Group gap="xs">
              <Text>{m.username}</Text>
              {m.is_owner && <Badge size="xs" color="blue">{t('members.ownerLabel')}</Badge>}
              <Badge size="xs" color={isOnline ? 'green' : 'gray'} variant="dot">
                {isOnline ? t('members.online') : t('members.offline')}
              </Badge>
            </Group>
            {!workspace.is_mirror && !m.is_owner && (
              <Button size="xs" variant="subtle" color="red" onClick={() => handleRemove(m.device_id)}>
                {t('members.removeButton')}
              </Button>
            )}
          </Group>
        );
      })}
    </Stack>
  );
}
```

- [ ] **Step 18.4: Typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 18.5: Commit**

```powershell
git add ui/src/components/workspace/WorkspaceMembersPanel.tsx ui/src/ipc/commands.ts crates/winx-kvm/src/commands/workspace.rs crates/winx-kvm/src/lib.rs
git commit -m "feat(workspace/ui): WorkspaceMembersPanel with online/offline status"
```

---

## Task 19 — Componente `WorkspaceDetailDrawer`

**Files:**
- Create: `ui/src/components/workspace/WorkspaceDetailDrawer.tsx`
- Modify: `ui/src/components/workspace/WorkspacesPanel.tsx` (integrar)

- [ ] **Step 19.1: Criar o drawer**

```tsx
import { useState } from 'react';
import { Button, Divider, Drawer, Group, Stack, TextInput, Title } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import WorkspaceMembersPanel from './WorkspaceMembersPanel';
import CreateWorkspaceModal from './CreateWorkspaceModal';
import type { WorkspaceDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto | null;
  onClose: () => void;
}

export default function WorkspaceDetailDrawer({ workspace, onClose }: Props) {
  const { t } = useTranslation('workspace');
  const [newName, setNewName] = useState('');
  const [inviteOpen, setInviteOpen] = useState(false);

  if (!workspace) return null;

  const handleRename = async () => {
    if (!newName.trim()) return;
    await ipc.renameWorkspace(workspace.id, newName.trim()).catch(console.error);
    setNewName('');
  };

  const handleDelete = async () => {
    await ipc.deleteWorkspace(workspace.id).catch(console.error);
    onClose();
  };

  const handleForget = async () => {
    await ipc.forgetWorkspace(workspace.id).catch(console.error);
    onClose();
  };

  return (
    <Drawer opened={!!workspace} onClose={onClose} title={workspace.name} position="right" size="md">
      <Stack gap="md">
        {!workspace.is_mirror && (
          <Group>
            <TextInput
              placeholder={t('detail.renamePlaceholder')}
              value={newName}
              onChange={(e) => setNewName(e.currentTarget.value)}
              style={{ flex: 1 }}
            />
            <Button onClick={handleRename}>{t('detail.renameButton')}</Button>
          </Group>
        )}

        <Divider />

        <WorkspaceMembersPanel workspace={workspace} onInviteClick={() => setInviteOpen(true)} />

        <Divider />

        <Group>
          {workspace.is_mirror ? (
            <Button color="orange" variant="light" onClick={handleForget}>
              {t('detail.forgetButton')}
            </Button>
          ) : (
            <Button color="red" variant="light" onClick={handleDelete}>
              {t('detail.deleteButton')}
            </Button>
          )}
        </Group>
      </Stack>

      <CreateWorkspaceModal
        opened={inviteOpen}
        onClose={() => setInviteOpen(false)}
        prefillWorkspaceId={workspace.id}
      />
    </Drawer>
  );
}
```

Nota: `CreateWorkspaceModal` precisa aceitar prop opcional `prefillWorkspaceId` para o caso de "convidar pra workspace existente". Se o modal atual só cria, criar um `InvitePeerModal` separado.

- [ ] **Step 19.2: Criar `InvitePeerModal` simples (mais escopo certo que reusar Create)**

`ui/src/components/workspace/InvitePeerModal.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { Button, Checkbox, Modal, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import type { DiscoveredPeer } from '../../ipc/commands';

interface Props {
  workspaceId: string | null;
  onClose: () => void;
}

export default function InvitePeerModal({ workspaceId, onClose }: Props) {
  const { t } = useTranslation('workspace');
  const [peers, setPeers] = useState<DiscoveredPeer[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (workspaceId) {
      ipc.listDiscoveredPeers().then(setPeers).catch(console.error);
    }
  }, [workspaceId]);

  const handleInvite = async () => {
    if (!workspaceId) return;
    for (const peerId of selected) {
      await ipc.inviteToWorkspace(workspaceId, peerId).catch(console.error);
    }
    onClose();
  };

  return (
    <Modal opened={!!workspaceId} onClose={onClose} title={t('invitePeer.title')}>
      <Stack gap="xs">
        {peers.length === 0 ? (
          <Text c="dimmed">{t('invitePeer.noPeers')}</Text>
        ) : (
          peers.map((p) => (
            <Checkbox
              key={p.id}
              label={p.username}
              checked={selected.has(p.id)}
              onChange={(e) => {
                const next = new Set(selected);
                if (e.currentTarget.checked) next.add(p.id);
                else next.delete(p.id);
                setSelected(next);
              }}
            />
          ))
        )}
        <Button onClick={handleInvite} disabled={selected.size === 0}>
          {t('invitePeer.inviteButton', { count: selected.size })}
        </Button>
      </Stack>
    </Modal>
  );
}
```

Atualizar o `Drawer` para usar `InvitePeerModal` no lugar de `CreateWorkspaceModal`:

```tsx
import InvitePeerModal from './InvitePeerModal';
// ... e troca a JSX no final:
<InvitePeerModal workspaceId={inviteOpen ? workspace.id : null} onClose={() => setInviteOpen(false)} />
```

- [ ] **Step 19.3: Integrar drawer no `WorkspacesPanel`**

Em `ui/src/components/workspace/WorkspacesPanel.tsx`, adicionar state para a seleção:

```tsx
const [detailWs, setDetailWs] = useState<WorkspaceDto | null>(null);
// ... no JSX onde renderiza WorkspaceCard:
<WorkspaceCard
  key={ws.id}
  workspace={ws}
  onOpenDetail={() => setDetailWs(ws)}
/>
// ... e no final:
<WorkspaceDetailDrawer workspace={detailWs} onClose={() => setDetailWs(null)} />
```

- [ ] **Step 19.4: Typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 19.5: Commit**

```powershell
git add ui/src/components/workspace/WorkspaceDetailDrawer.tsx ui/src/components/workspace/InvitePeerModal.tsx ui/src/components/workspace/WorkspacesPanel.tsx
git commit -m "feat(workspace/ui): WorkspaceDetailDrawer with members panel and invite"
```

---

## Task 20 — Toast no `workspace-sync-applied`

**Files:**
- Modify: `ui/src/App.tsx` (ou onde o listener global vive)

- [ ] **Step 20.1: Adicionar import de `notifications` do Mantine**

```tsx
import { notifications } from '@mantine/notifications';
```

Se `@mantine/notifications` ainda não estiver instalado, confirmar com `package.json` — Sprint W2 não usou. Caso falte, **não** instalar nova dep nessa task (Sprint W3 não muda stack); usar uma alternativa simples:

```tsx
// In-house toast via state in store, ou um console.info temporário
useWorkspaceStore.getState().addToast(...);
```

Se `@mantine/notifications` está disponível, usar:

```tsx
case 'workspaces-updated':
case 'workspace-sync-applied': {
  // refresh + toast
  refreshWorkspaces();
  if (payload.kind === 'workspace-sync-applied' && payload.workspace_id) {
    notifications.show({
      title: t('toast.syncApplied.title'),
      message: t('toast.syncApplied.message', { workspaceId: payload.workspace_id }),
      color: 'blue',
    });
  }
  break;
}
```

- [ ] **Step 20.2: Verificar/adicionar `@mantine/notifications` ao `package.json`**

```powershell
Set-Location ui; pnpm list @mantine/notifications
```

Se ausente, **PARAR e perguntar ao usuário** se pode adicionar a dep (stack travada). Caso afirmativo:

```powershell
pnpm add @mantine/notifications@9.0.0
```

E adicionar `<Notifications />` no root da app.

- [ ] **Step 20.3: Typecheck**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 20.4: Commit**

```powershell
git add ui/src/App.tsx ui/package.json ui/pnpm-lock.yaml
git commit -m "feat(workspace/ui): toast on remote sync applied"
```

---

## Task 21 — Adicionar i18n keys

**Files:**
- Modify: `ui/src/i18n/locales/en/workspace.json`
- Modify: `ui/src/i18n/locales/pt-BR/workspace.json`

- [ ] **Step 21.1: Adicionar chaves em `en/workspace.json`**

Mergear no objeto raiz:

```json
{
  "card": {
    "available": "Available",
    "unavailable": "Unavailable",
    "orphanBadge": "Orphan",
    "orphanTooltip": "Owner deleted this workspace. You can keep your copy or forget it.",
    "forgetButton": "Forget"
  },
  "detail": {
    "renamePlaceholder": "New workspace name",
    "renameButton": "Rename",
    "deleteButton": "Delete workspace",
    "forgetButton": "Forget workspace"
  },
  "members": {
    "title": "Members",
    "inviteButton": "Invite",
    "removeButton": "Remove",
    "ownerLabel": "Owner",
    "online": "Online",
    "offline": "Offline"
  },
  "invitePeer": {
    "title": "Invite peers",
    "noPeers": "No peers discovered on the network",
    "inviteButton_zero": "Invite",
    "inviteButton_one": "Invite 1 peer",
    "inviteButton_other": "Invite {{count}} peers"
  },
  "toast": {
    "syncApplied": {
      "title": "Workspace updated",
      "message": "Changes applied"
    }
  }
}
```

- [ ] **Step 21.2: Adicionar mesmas chaves em `pt-BR/workspace.json`**

```json
{
  "card": {
    "available": "Disponível",
    "unavailable": "Indisponível",
    "orphanBadge": "Órfão",
    "orphanTooltip": "O owner deletou esse workspace. Você pode manter a cópia ou esquecer.",
    "forgetButton": "Esquecer"
  },
  "detail": {
    "renamePlaceholder": "Novo nome do workspace",
    "renameButton": "Renomear",
    "deleteButton": "Deletar workspace",
    "forgetButton": "Esquecer workspace"
  },
  "members": {
    "title": "Membros",
    "inviteButton": "Convidar",
    "removeButton": "Remover",
    "ownerLabel": "Owner",
    "online": "Online",
    "offline": "Offline"
  },
  "invitePeer": {
    "title": "Convidar peers",
    "noPeers": "Nenhum peer descoberto na rede",
    "inviteButton_zero": "Convidar",
    "inviteButton_one": "Convidar 1 peer",
    "inviteButton_other": "Convidar {{count}} peers"
  },
  "toast": {
    "syncApplied": {
      "title": "Workspace atualizado",
      "message": "Mudanças aplicadas"
    }
  }
}
```

- [ ] **Step 21.3: Rodar typecheck e CI de i18n (se houver)**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found". Se houver script `pnpm i18n:check`, rodar também.

- [ ] **Step 21.4: Commit**

```powershell
git add ui/src/i18n/locales/en/workspace.json ui/src/i18n/locales/pt-BR/workspace.json
git commit -m "feat(workspace/ui): i18n keys for W3 (detail, members, orphan, toast)"
```

---

## Task 22 — Atualizar `Workspace-TODO.md` para marcar W3 como feito

**Files:**
- Modify: `Workspace-TODO.md`

- [ ] **Step 22.1: Trocar checkboxes da Sprint W3**

Trocar `- [ ]` por `- [x]` em todos os itens W3.1 até W3.14.

- [ ] **Step 22.2: Atualizar status no header**

```markdown
> **Status**: Sprints W1, W2 e W3 ✅ concluídos. Próximo: W4 (Global cursor + Layout + Polish).
> **Última atualização**: 2026-05-22
```

- [ ] **Step 22.3: Commit**

```powershell
git add Workspace-TODO.md
git commit -m "docs(workspace): mark Sprint W3 as completed"
```

---

## Task 23 — Verificação final

- [ ] **Step 23.1: Rodar fmt + clippy + todos os testes Rust**

```powershell
cargo fmt --all
cargo clippy -p winx-application -p winx-infra -p winx-protocol --all-targets
cargo test --workspace
```

Expected: 0 erros novos em clippy, todos os testes passam.

- [ ] **Step 23.2: Rodar typecheck do frontend**

```powershell
Set-Location ui; pnpm tsc --noEmit
```

Expected: "No errors found".

- [ ] **Step 23.3: Smoke manual com 2 instâncias localhost** (W3.13 verification)

Em duas janelas PowerShell:

```powershell
# Janela 1 (Desktop):
$env:APPDATA = "C:\Users\kingdaswinx\AppData\Roaming\Winx-KVM-Test-A"
cargo tauri dev

# Janela 2 (Notebook simulado):
$env:APPDATA = "C:\Users\kingdaswinx\AppData\Roaming\Winx-KVM-Test-B"
cargo tauri dev
```

Checklist manual:
- [ ] A cria workspace "Sala" e convida B
- [ ] B aceita; mirror aparece com badge "Convidado por <A>"
- [ ] A renomeia "Sala" para "Sala-Renomeada"
- [ ] **Esperado:** Toast aparece em B; nome do mirror atualiza pra "Sala-Renomeada"; badge "Disponível" verde
- [ ] A fecha o app (Alt+F4)
- [ ] **Esperado:** após 30s, badge em B vira "Indisponível" (cinza)
- [ ] A reabre, mirror em B volta pra "Disponível"
- [ ] A deleta "Sala-Renomeada"
- [ ] **Esperado:** mirror em B fica com badge "Órfão" laranja; botão "Esquecer" aparece
- [ ] B clica "Esquecer"; workspace some da lista

- [ ] **Step 23.4: Commit final (se houver mudanças)**

```powershell
git status
# Se ainda houver arquivos modificados (ex: ajustes de smoke test), commitar.
```

---

## Tasks summary

| # | Task | Camada | Tipo |
|---|---|---|---|
| 1 | Payloads Sync/Delete no protocol | protocol+infra | Code |
| 2 | Error code `WorkspaceNotOwner` | domain | Code |
| 3 | Event `MemberPresenceChanged` | domain | Code |
| 4 | Enum `WorkspacePatch` | application | Code |
| 5 | `update_workspace` + broadcast | application | Code |
| 6 | Testes E2E `update_workspace` | application | Test |
| 7 | `delete_workspace` notifica membros | application | Code |
| 8 | `forget_workspace` | application | Code |
| 9 | `handle_workspace_sync` (LWW) | application | Code+Test |
| 10 | `handle_workspace_delete` (orphan) | application | Code+Test |
| 11 | `run_presence_watcher` | application | Code |
| 12 | Tauri commands novos | kvm | Code |
| 13 | Mapping de eventos pro UI | kvm | Code |
| 14 | Spawn presence_watcher | kvm | Code |
| 15 | IPC wrappers frontend | ui | Code |
| 16 | `workspaceStore` + presence | ui | Code |
| 17 | `WorkspaceCard` badges | ui+kvm | Code |
| 18 | `WorkspaceMembersPanel` | ui+kvm | Code |
| 19 | `WorkspaceDetailDrawer` + InvitePeerModal | ui | Code |
| 20 | Toast no sync | ui | Code |
| 21 | i18n keys | ui | Code |
| 22 | Atualizar Workspace-TODO.md | docs | Doc |
| 23 | Verificação final + smoke | meta | Validation |

**Estimativa total:** ~23h (alinhado com 20-30h previstos no Workspace-TODO.md para Sprint W3).
