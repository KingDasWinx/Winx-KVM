# Workspace-TODO.md

Plano executável da feature **Workspaces** do Winx-KVM. Complementa o [README](README.md) e o [docs/PLANNING.md](docs/PLANNING.md) com o backlog específico do 8º bounded context (`workspace`).

> **Status**: Especificação aprovada. Aguardando início do Sprint W1.
> **Última atualização**: 2026-05-21

---

## Sumário

- [Visão da feature](#visão-da-feature)
- [Decisões aprovadas](#decisões-aprovadas)
- [Arquitetura DDD](#arquitetura-ddd)
- [Modelo de dados](#modelo-de-dados)
- [Fluxos principais](#fluxos-principais)
- [Backlog por sprint](#backlog-por-sprint)
- [Critérios de qualidade](#critérios-de-qualidade)
- [Riscos específicos](#riscos-específicos)
- [Pontos abertos](#pontos-abertos)

---

## Visão da feature

Hoje o Winx-KVM só permite conexão 1:1 entre dois PCs via pareamento Ed25519. **Workspace** é um agrupamento persistente de N devices na mesma LAN onde:

- Mouse e teclado funcionam de forma **global** — uma posição X/Y única é compartilhada entre todos os PCs do workspace, independente de qual está controlando no momento
- Layout de monitores fica **salvo por workspace** (cada workspace pode ter sua própria disposição)
- **Qualquer membro** pode convidar novos devices, editar membros e ajustar o layout (modelo colaborativo, sem owner privilegiado em runtime)
- O criador (owner) é só o "publisher" inicial — convidados recebem uma **cópia (mirror)** que persiste mesmo se o owner sair ou deletar o workspace
- **TOFU** (Trust On First Use): aceitar um invite cria automaticamente o trust Ed25519, sem PIN
- **LAN-only** (sem servidor cloud, mantém o princípio do produto inteiro)

### Regra de exclusividade

Um device só pode estar **ativamente conectado** (com input control rodando) a **1 workspace por vez**. Pode ser membro de N. Tentar conectar a outro workspace ou peer enquanto já conectado abre o **ConflictModal** com botão grande "Criar workspace" que pré-popula os 2 PCs em jogo.

---

## Decisões aprovadas

| Tópico | Decisão |
|---|---|
| Pareamento prévio | **Não exigido.** Aceitar invite faz TOFU implícito (cria trust em `IdentityStore::trust_peer()`) |
| Identidade do invite | `DeviceId + PublicKey` (username é só label visual cacheado) |
| Permissões | **Colaborativo** — qualquer membro edita |
| Modal de conflito | Botão grande "Criar workspace" pré-popula com os 2 PCs em jogo |
| Sync do mouse global X/Y | **Real-time** via stream Control do QUIC (throttle 60Hz) + debounce 1s para persistir |
| Owner deleta workspace | Mirror persiste com badge **"Órfão"** |
| Visual da cópia/mirror | Badge "Convidado por &lt;username&gt;" + cor de borda secundária |
| Storage | `%APPDATA%\Winx-KVM\workspaces.toml` único |
| Escopo da regra "1 conectado" | Por DeviceId (não existe UserAccount no sistema) |
| Conflito colaborativo | LWW (Last-Write-Wins) por `version: u64` global do workspace |
| Atualização do mirror | Automática (não pede confirmação) |
| Múltiplos devices "do mesmo user" | Permitido — sistema opera em devices |

---

## Arquitetura DDD

### 8º bounded context: `workspace`

```
crates/winx-domain/src/workspace/
├── mod.rs                  # re-exports públicos
├── workspace.rs            # Aggregate Workspace (root)
├── workspace_id.rs         # WorkspaceId (UUID v4 newtype)
├── member.rs               # WorkspaceMember (DeviceId, PublicKey, username_cache, joined_at)
├── invite.rs               # InviteSession + state machine
├── layout.rs               # WorkspaceLayout (posições de monitores por device)
├── global_cursor.rs        # GlobalCursorState
├── connection.rs           # WorkspaceConnection (qual device controla localmente)
├── ownership.rs            # OwnershipMode { Original | Mirror{...} }
├── version.rs              # WorkspaceVersion (u64 monotônico para LWW)
└── events.rs               # Domain events
```

**Regra de dependência inviolável** (igual o resto do projeto): `winx-domain` puro, sem `tokio`, sem rede. Adapters concretos em `winx-infra`.

### Mapa de arquivos

| Camada | Arquivos novos | Modificar |
|---|---|---|
| `winx-domain` | 10 arquivos em `src/workspace/` | `src/lib.rs` |
| `winx-application` | `ports/workspace.rs`, `use_cases/workspace.rs` | `lib.rs` |
| `winx-protocol` | `src/workspace.rs` | `src/lib.rs` (Payload enum + PROTOCOL_VERSION bump) |
| `winx-infra` | `workspace_store.rs`, `workspace_transport.rs` | `lib.rs` |
| `winx-kvm` (Tauri) | `commands/workspace.rs` | `commands/mod.rs`, `events/mod.rs`, `app_state.rs` |
| `ui/` | 8 componentes em `components/workspace/`, 4 hooks, `store/workspaceStore.ts`, 2 i18n JSON | `pages/HomePage.tsx`, `ipc/commands.ts`, `ipc/events.ts` |

### Conceito-chave: `OwnershipMode`

```rust
pub enum OwnershipMode {
    Original,
    Mirror {
        owner_device_id: DeviceId,
        owner_username_snapshot: String,
        owner_last_seen: SystemTime,
        is_orphan: bool,  // true quando owner deletou
    },
}
```

Mirror é uma cópia local persistida em `workspaces.toml` igual ao Original. Sync automático via stream Control quando o owner está online. Quando owner deleta → manda `WorkspaceDelete` → mirror marca `is_orphan = true` mas **continua na lista**.

---

## Modelo de dados

### Aggregate `Workspace`

```rust
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_device_id: DeviceId,
    pub version: WorkspaceVersion,  // u64 monotônico
    pub ownership_mode: OwnershipMode,
    pub members: Vec<WorkspaceMember>,
    pub layout: WorkspaceLayout,
    pub global_cursor: GlobalCursorState,
    pub created_at: SystemTime,
}
```

**Invariants**:
- `name` não pode ser vazio
- `owner_device_id` deve aparecer em `members`
- `version` só pode crescer
- Mirror não pode ser editado localmente exceto pelo handler de sync

### State machine `InviteSession`

```rust
pub enum InviteState {
    Pending { invite_id, target_device_id, expires_at },  // 90s
    Delivered,
    Accepted { accepted_at },
    Rejected,
    Expired,
}
```

### Storage TOML

`%APPDATA%\Winx-KVM\workspaces.toml`:

```toml
schema_version = 1

[[workspace]]
id = "550e8400-e29b-41d4-a716-446655440000"
name = "Sala"
owner_device_id = "..."
version = 17
ownership_mode = "original"  # ou "mirror"

[workspace.global_cursor]
x = 1920
y = 540
active_device_id = "..."
monotonic_seq = 421
last_seen_at = 1747876543

[[workspace.members]]
device_id = "..."
public_key = "base64-ed25519"
username_cache = "João-Desktop"
joined_at = 1747800000

[workspace.layout]
# posições de monitores per-device (formato definido no Sprint W1)

# Quando ownership_mode = "mirror":
# [workspace.mirror_info]
# owner_device_id = "..."
# owner_username_snapshot = "João"
# owner_last_seen = 1747876000
# is_orphan = false
```

---

## Fluxos principais

### Fluxo 1 — Criar workspace + convidar

1. User clica "Novo workspace" → `CreateWorkspaceModal` (nome + checkboxes de peers descobertos)
2. Submit → Tauri command `create_workspace(name, initial_members)`
3. Use case cria `Workspace { ownership_mode: Original, version: 1 }`, salva via `WorkspaceStore`
4. Para cada `initial_member`: chama `invite_to_workspace`
5. `WorkspaceTransport::send_invite` abre stream Control e envia `WorkspaceInvitePayload`
6. Frontend emite event `workspaces-updated`

### Fluxo 2 — Receber invite (TOFU)

1. Stream Control no peer recebe `WorkspaceInvitePayload`
2. Backend emite event Tauri `workspace-invite-received`
3. Frontend abre `IncomingInviteModal` no **centro da tela** (sem PIN, só Aceitar/Recusar; mostra fingerprint do sender pra UX educativa)
4. **Aceitar**: `accept_invite(invite_id)`
   - Se sender não está em trusted_peers → `IdentityStore::trust_peer(public_key)` (TOFU implícito)
   - Cria `Workspace { ownership_mode: Mirror{...}, ... }`, salva via store
   - Envia `WorkspaceInviteResponse{accepted: true}` de volta
5. **Recusar**: `reject_invite(invite_id)` → envia response negativa

### Fluxo 3 — Conectar com conflito

1. User clica "Conectar" em workspace W2 mas já está em W1
2. `connect_to_workspace(W2)` retorna `Err(WorkspaceConflict { active_id: W1, target: W2 })`
3. Frontend abre `ConflictModal`:
   - "Você já está conectado a &lt;W1.name&gt;..."
   - Botão "Sim, desconectar" → `force_disconnect_and_connect(W2)`
   - Botão "Não" → fecha modal
   - **Botão grande "Criar workspace"** → abre `CreateWorkspaceModal` com `initial_members = [device_atual_W1, device_alvo]` pré-marcados

### Fluxo 4 — Sync de mudanças (LWW)

1. Member A renomeia workspace → `update_workspace(id, patch{name: "Novo"})`
2. Use case incrementa `version`, salva localmente, chama `send_sync(targets=members, snapshot)`
3. Cada member recebe `WorkspaceSyncPayload` via stream Control
4. `handle_workspace_sync`: se `incoming.version > local.version` → sobrescreve; senão descarta
5. Toast no UI: "&lt;workspace_name&gt; foi atualizado"

### Fluxo 5 — Mouse global

1. Device A controla o workspace (tem `WorkspaceConnection::Active`)
2. `input_control::on_mouse_move(x, y)` → `publish_global_cursor(x, y)`
3. Use case faz throttle 60Hz → envia `GlobalCursorPayload` no stream Control
4. Devices B, C... recebem → `apply_remote_cursor` valida `monotonic_seq` ascendente → atualiza estado in-memory
5. Debounce 1s persiste `GlobalCursorState` em `workspaces.toml`
6. Quando B assume controle (troca de foco no workspace) → lê último `x, y` conhecido pra retomar a posição exata

---

## Backlog por sprint

### Sprint W1 — Domain + storage

**Esforço**: 15–20h | **Risco**: Baixo
**Critério de aceite**: testes unitários do domínio passam, persistência em TOML round-trip funciona.

- [ ] **W1.1** Criar `crates/winx-domain/src/workspace/` com módulos `workspace_id.rs`, `member.rs`, `version.rs`, `ownership.rs`, `events.rs`
- [ ] **W1.2** Aggregate `Workspace` com invariants: name non-empty, version monotonic, owner_device_id ∈ members
- [ ] **W1.3** Value object `WorkspaceLayout` (espelha `MonitorLayout` existente em `input_control::layout`)
- [ ] **W1.4** Enum `OwnershipMode { Original, Mirror{...} }` + métodos `is_mirror()`, `mark_orphan()`
- [ ] **W1.5** `GlobalCursorState::apply_remote(payload)` valida `monotonic_seq` ascendente
- [ ] **W1.6** State machine `InviteSession` com testes de cada transição (Pending→Delivered→Accepted, expiração 90s)
- [ ] **W1.7** Domain events: `WorkspaceCreated`, `WorkspaceUpdated`, `WorkspaceDeleted`, `MemberJoined`, `MemberLeft`, `InviteSent`, `InviteAccepted`, `InviteRejected`, `GlobalCursorMoved`
- [ ] **W1.8** Port `WorkspaceStore` em `winx-application/src/ports/workspace.rs`
- [ ] **W1.9** Adapter `WorkspaceTomlStore` em `winx-infra/` (reusar padrão de `PeersTomlStore`)
- [ ] **W1.10** Testes: round-trip TOML, schema compatibility, fixtures com mirror + original

---

### Sprint W2 — Invite + TOFU + Conflict

**Esforço**: 20–25h | **Risco**: Médio
**Critério de aceite**: invite chega no centro da tela do peer, aceitar grava mirror + trust automático, modal de conflito aparece quando tenta conectar com já-conectado.

- [ ] **W2.1** Adicionar payloads em `winx-protocol/src/workspace.rs` (`WorkspaceInvitePayload`, `WorkspaceInviteResponse`, `MemberSnapshot`, `WorkspaceSnapshot`) + bump `PROTOCOL_VERSION`
- [ ] **W2.2** Port `WorkspaceTransport::send_invite` + `send_invite_response` + `subscribe()`
- [ ] **W2.3** Adapter `QuicWorkspaceTransport` em winx-infra reusando o `QuicTransport`
- [ ] **W2.4** Use case `invite_to_workspace`: cria `InviteSession`, envia payload via transport
- [ ] **W2.5** Use case `accept_invite`: TOFU via `IdentityStore::trust_peer` se necessário + grava Mirror no store + envia response
- [ ] **W2.6** Use case `reject_invite` + handler de expiração 90s
- [ ] **W2.7** `active_workspace: RwLock<Option<WorkspaceId>>` em `app_state.rs`
- [ ] **W2.8** Detecção de conflito: `connect_to_workspace` retorna `Err(WorkspaceConflict)` se já conectado
- [ ] **W2.9** Tauri commands: `list_workspaces`, `create_workspace`, `delete_workspace`, `invite_to_workspace`, `accept_invite`, `reject_invite`, `connect_to_workspace`, `disconnect_from_workspace`, `force_disconnect_and_connect`
- [ ] **W2.10** Eventos Tauri: `workspaces-updated`, `workspace-invite-received`, `workspace-connection-conflict`
- [ ] **W2.11** UI `IncomingInviteModal` (centro da tela, sem PIN, Aceitar/Recusar; mostra fingerprint do sender)
- [ ] **W2.12** UI `ConflictModal` com botão grande "Criar workspace" pré-populando `initial_members = [active, target]`
- [ ] **W2.13** UI `CreateWorkspaceModal` (wizard: nome + checkboxes de peers descobertos)
- [ ] **W2.14** UI `WorkspaceCard` básico (sem badge mirror ainda) integrado na `HomePage`
- [ ] **W2.15** Namespace i18n `workspace.json` em `en/` + `pt-BR/`
- [ ] **W2.16** Testes integração: invite end-to-end com 2 instâncias localhost

---

### Sprint W3 — Sync, Mirror, Órfão

**Esforço**: 20–30h | **Risco**: Médio-Alto
**Critério de aceite**: edits do owner propagam pra mirrors automaticamente; deletar workspace marca mirror como Órfão com badge; LWW resolve conflitos de edits concorrentes.

- [ ] **W3.1** Payloads `WorkspaceSyncPayload` + `WorkspaceDeletePayload` no protocol
- [ ] **W3.2** Use case `update_workspace(id, patch)` incrementa `version` + chama `send_sync` para todos os membros
- [ ] **W3.3** Handler `handle_workspace_sync`: aplica LWW (`incoming.version > local.version`), salva no store, emite `workspaces-updated`
- [ ] **W3.4** Use case `delete_workspace`: só owners; envia `WorkspaceDelete` antes de remover localmente
- [ ] **W3.5** Handler `handle_workspace_delete`: marca `is_orphan = true` no mirror, **não** remove
- [ ] **W3.6** Watcher de `owner_last_seen`: se mirror não recebe sync/heartbeat > 30s → emite event `member-presence-changed`
- [ ] **W3.7** UI: badge "Convidado por &lt;username&gt;" em mirrors + cor de borda secundária
- [ ] **W3.8** UI: badge "Órfão" + tooltip explicativo quando `is_orphan = true`
- [ ] **W3.9** UI: tag "Disponível" (verde) / "Indisponível" (cinza) baseado em `>= 1 membro online`
- [ ] **W3.10** UI `WorkspaceMembersPanel`: adicionar/remover membros, status online/offline, botão "Convidar"
- [ ] **W3.11** UI `WorkspaceDetailDrawer`: integra members panel + delete + leave
- [ ] **W3.12** Testes: split-brain (2 edits offline, reconnect, LWW resolve)
- [ ] **W3.13** Toast Mantine quando workspace recebe update remoto ("&lt;workspace_name&gt; foi atualizado")
- [ ] **W3.14** "Esquecer este workspace" no UI pra mirrors órfãos (mitigação WR3)

---

### Sprint W4 — Global cursor + Layout + Polish

**Esforço**: 25–35h | **Risco**: Alto
**Critério de aceite**: mouse global X/Y replica entre devices em tempo real; layout de monitores editável por workspace; transição de controle entre devices preserva posição.

- [ ] **W4.1** `GlobalCursorPayload` no protocol + `WorkspaceTransport::send_global_cursor`
- [ ] **W4.2** Use case `publish_global_cursor(x, y)` com throttle 60Hz (16ms) — invocado pelo `input_control` quando este device tem controle ativo
- [ ] **W4.3** Use case `apply_remote_cursor(payload)` valida `monotonic_seq` ascendente, atualiza estado in-memory
- [ ] **W4.4** Debounce 1s pra persistir `GlobalCursorState` em `workspaces.toml`
- [ ] **W4.5** Integração no `input_control::focus`: quando este device assume controle, lê `last x, y` do `GlobalCursorState` pra retomar posição
- [ ] **W4.6** UI `WorkspaceLayoutEditor`: drag-and-drop de monitores numa grade, salva via `update_workspace(patch.layout = ...)`
- [ ] **W4.7** Hook `useGlobalCursor` no UI (opcional, debug-only — mostra X/Y live no header)
- [ ] **W4.8** Hotkey opcional `Ctrl+Alt+W` pra abrir o workspace ativo (configurável)
- [ ] **W4.9** Testes integração: 3 instâncias localhost; mouse no PC A → controle passa pra PC B → posição retomada corretamente
- [ ] **W4.10** Benchmark de latência do global cursor (< 10ms p99 em loopback)
- [ ] **W4.11** Polish: loading states, error toasts via Mantine, animações suaves de transição
- [ ] **W4.12** Atualizar `docs/PLANNING.md` adicionando Épico 10 (Workspaces) marcado como done
- [ ] **W4.13** Smoke test em 2 PCs físicos (Desktop + Notebook) por 30min sem crash

---

## Critérios de qualidade

A barra para considerar a feature pronta (espelha `docs/PLANNING.md`):

- [ ] Cobertura > 80% em `winx-domain::workspace`
- [ ] Pelo menos 1 teste de integração por use case em `winx-application::use_cases::workspace`
- [ ] Latência p99 do global cursor < 10ms em loopback
- [ ] Sem panic em 1h de uso ativo de workspace
- [ ] `cargo clippy --all-targets -- -D warnings` limpo
- [ ] `cd ui; pnpm tsc --noEmit && pnpm lint` sem erros
- [ ] CI de i18n passa (toda chave em `en/workspace.json` existe em `pt-BR/workspace.json`)
- [ ] Documentação inline `///` em todas as funções públicas de use_cases e ports

---

## Riscos específicos

| # | Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|---|
| WR1 | Sync de cursor 60Hz satura banda em workspaces grandes (5+ PCs) | Média | Médio | Throttle adaptativo: 60Hz só pro device ativo; outros 10Hz; trocar pra QUIC datagrams (unreliable) se necessário |
| WR2 | LWW perde edits simultâneos de 2 membros | Média | Baixo | Toast informativo ao receber sync com version maior, sugerindo refresh |
| WR3 | Mirror fica órfão "fantasma" para sempre se owner nunca volta | Alta | Baixo | UI permite deletar mirror manualmente ("Esquecer este workspace") |
| WR4 | Race entre múltiplos invites simultâneos pro mesmo target | Baixa | Baixo | `InviteSession` com unique invite_id; segundo invite substitui o primeiro |
| WR5 | Conflito quando 2 membros tentam controle simultâneo no mesmo workspace | Média | Alto | Reusar `FocusState` Mutex existente em `input_control` (já single-writer) |
| WR6 | Schema de `workspaces.toml` muda em versão futura | Média | Médio | `schema_version` no topo + migrações em `WorkspaceTomlStore::load_all` |
| WR7 | Auto-pareamento TOFU expõe a ataques de spoofing de PC malicioso na LAN | Média | Alto | `IncomingInviteModal` mostra fingerprint Ed25519 do sender (UX educativa, sem bloquear MVP) |

---

## Pontos abertos

Itens que precisam de decisão durante a implementação:

### Durante Sprint W1

1. **Formato exato de `WorkspaceLayout`**: copiar 1:1 do `MonitorLayout` existente em `input_control::layout`, ou criar estrutura nova com `HashMap<DeviceId, MonitorLayout>`?
2. **Hash do estado para detectar drift**: vale incluir um `state_hash` no Workspace pra detectar divergências entre mirrors? (Provavelmente não no MVP)

### Durante Sprint W2

3. **Reusar `pairing::PairingSession` infrastructure** ou criar `InviteSession` totalmente novo? (Sugestão: separado por clareza de domínio)
4. **Comportamento se target estiver offline no momento do invite**: descartar ou fila pra retry? (Sugestão MVP: descartar e mostrar erro no UI)
5. **UI da fingerprint no IncomingInviteModal**: hex completo ou short SHA + emoji visual (estilo SSH fingerprint)?

### Durante Sprint W3

6. **Membros podem se auto-remover ("leave workspace")?** Sugestão: sim, vira invite para o owner reconvidar
7. **Conflito de remoção concorrente** (A remove B, simultaneamente B remove A): LWW vai resolver mas pode dar UX estranha
8. **Versionamento de `WorkspaceLayout`**: layout faz parte do snapshot único ou tem versão própria pra evitar conflito com renome?

### Durante Sprint W4

9. **Estratégia de stream pro GlobalCursorPayload**: stream Control existente ou um stream/datagram novo dedicado? (Performance vs simplicidade)
10. **Comportamento quando o device com controle ativo cai abruptamente**: timeout pra liberar controle? (Sugestão: 5s sem heartbeat → libera)

### Pós-MVP

11. **Persistência cross-restart do `active_workspace`**: ao abrir o app, reconectar automaticamente ao último workspace?
12. **Discovery de "workspaces na rede"**: além de mDNS de devices, anunciar workspaces ativos pra facilitar descoberta?
13. **Auditoria de mudanças**: log de quem editou o quê (útil em workspaces com muitos membros)?

---

## Próximos passos imediatos

1. Validar este TODO com o time (José).
2. Atualizar `docs/PLANNING.md` adicionando referência ao Épico 10 (Workspaces) e linkando para este arquivo.
3. Começar **W1.1**: scaffolding dos módulos em `crates/winx-domain/src/workspace/`.
