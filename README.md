# Winx-KVM

> Software KVM (Keyboard, Video, Mouse) com áudio compartilhado entre PCs Windows na mesma rede — escrito em Rust, embalado em Tauri, com UI em React.

[![Status](https://img.shields.io/badge/status-planejamento-yellow)](#roadmap)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2B-0078D6)](#requisitos)

---

## Sumário

- [Visão](#visão)
- [Funcionalidades](#funcionalidades)
- [Stack Tecnológica](#stack-tecnológica)
- [Arquitetura](#arquitetura)
- [Modelo de Domínio (DDD)](#modelo-de-domínio-ddd)
- [Estrutura de Pastas](#estrutura-de-pastas)
- [Protocolo de Comunicação](#protocolo-de-comunicação)
- [Modelo de Segurança](#modelo-de-segurança)
- [Configuração e Persistência](#configuração-e-persistência)
- [Roadmap](#roadmap)
- [Setup de Desenvolvimento](#setup-de-desenvolvimento)
- [Decisões Arquiteturais (ADRs)](#decisões-arquiteturais-adrs)
- [Glossário](#glossário)

---

## Visão

**Winx-KVM** permite que dois (ou mais) PCs Windows compartilhem mouse, teclado, clipboard, arquivos e áudio como se fossem um único computador estendido. Mova o cursor pela borda direita do monitor e ele "passa" para o outro PC. Copie um arquivo no desktop e cole no notebook. Conecte o fone em apenas um PC e escute o áudio dos dois.

O app roda como **um único serviço idêntico** em cada máquina. Ao abrir, ele se anuncia na LAN; outros dispositivos com Winx-KVM aparecem na lista. Você envia uma solicitação de pareamento, um PIN de 6 dígitos aparece em uma das telas, e ao digitar no outro PC os dois ficam pareados — confiança persistente, sem precisar parear de novo.

Casos de uso primários:

- **Estação dual de produtividade**: desktop com 2 monitores + notebook com 1 monitor, todos controláveis com um teclado e um mouse físico.
- **Áudio único**: usar o mesmo fone para ouvir música do desktop e chamadas do notebook sem trocar de cabo.
- **Transferência fluida**: copiar texto, imagens e arquivos entre os PCs sem cloud ou pendrive.

---

## Funcionalidades

### v0.1 (MVP)

- [ ] Descoberta automática na LAN via mDNS/Bonjour
- [ ] Pareamento com PIN de 6 dígitos + troca de chaves Ed25519
- [ ] Confiança persistente após primeiro pareamento
- [ ] Transporte QUIC com TLS 1.3 (criptografia ponta-a-ponta)
- [ ] Compartilhamento de mouse e teclado entre dois PCs (1 monitor cada lado)
- [ ] Hotkey de pânico (`Ctrl+Alt+Home`) para forçar retorno do controle
- [ ] Hotkey de lock (`Scroll Lock`) para travar o cursor no PC atual
- [ ] Sincronização de clipboard de texto (UTF-8)
- [ ] UI básica: lista de peers, status de conexão, configurações

### v0.2 — Polimento Visual e Layout

- [ ] UI drag-drop para layout físico dos monitores entre PCs
- [ ] Auto-detecção de monitores via `EnumDisplayMonitors` e re-detecção em `WM_DISPLAYCHANGE`
- [ ] Clipboard de imagens (PNG) e RTF/HTML formatado
- [ ] Tray icon com menu de status e atalhos
- [ ] Tema claro/escuro

### v0.3 — Transferência de Arquivos

- [ ] Ctrl+C em arquivo no Explorer → Ctrl+V no outro PC dispara streaming
- [ ] Streams QUIC dedicados por arquivo (chunks de 64KB, hash SHA-256 final)
- [ ] UI de progresso por transferência com pausa/retomada
- [ ] `IDataObject` virtual com `CFSTR_FILECONTENTS` + `CFSTR_FILEGROUPDESCRIPTOR` para integração com o Explorer

### v0.4 — Áudio

- [ ] Detecção e validação da instalação do VB-Audio Cable
- [ ] Captura de áudio do dispositivo virtual com `cpal` (WASAPI exclusive)
- [ ] Codificação Opus 48kHz estéreo, frames de 10ms
- [ ] Stream QUIC dedicado para áudio com sequence numbers
- [ ] Buffer de jitter adaptativo (20–60ms) no receptor
- [ ] Roteamento bidirecional (output e mic virtual)

### v1.0 — Estável

- [ ] Installer MSI assinado
- [ ] Auto-update via Tauri Updater
- [ ] Telemetria opt-in (estatísticas anonimizadas de uso)
- [ ] Documentação completa do usuário e do contribuidor

### v2.0+ — Futuro

- [ ] Link USB-C/Thunderbolt como path alternativo de menor latência
- [ ] Suporte a Linux (X11/Wayland) e macOS
- [ ] Mais de 2 peers simultâneos (mesh)
- [ ] Driver de áudio virtual próprio (substituir VB-Cable)
- [ ] Serviço Windows com privilégios SYSTEM (controlar tela de logon)

---

## Stack Tecnológica

### Backend (Rust)

| Componente | Crate | Versão | Função |
|---|---|---|---|
| Runtime async | `tokio` | 1.x | Loop assíncrono, broadcast channel |
| Serialização | `serde` + `bincode` | 1.x / 2.x | Wire format binário |
| Transporte | `quinn` + `rustls` | 0.11 / 0.23 | QUIC + TLS 1.3 |
| Discovery | `mdns-sd` | 0.11 | Anúncio e descoberta na LAN |
| Crypto | `ring` + `ed25519-dalek` | 0.17 / 2.x | Chaves, assinaturas, KDF |
| Input | `windows` | 0.58 | Win32 hooks + SendInput |
| Áudio | `cpal` + `opus` | 0.15 / 0.3 | Captura WASAPI + codec |
| Clipboard | `arboard` | 3.x | Read/write clipboard nativo |
| Config | `toml` + `serde` | 0.8 | Serialização TOML |
| Credenciais | `keyring` | 3.x | Windows Credential Manager (DPAPI) |
| Logging | `tracing` + `tracing-subscriber` | 0.1 / 0.3 | Estruturado, com spans |
| Shell desktop | `tauri` | 2.11.1 | App nativo + IPC com React |

### Frontend

| Componente | Versão | Função |
|---|---|---|
| Tauri | 2.11.1 | Shell desktop, IPC, plugins |
| React | 19.2 | UI |
| Vite | 8.x | Bundler/dev server |
| TypeScript | 5.x | Tipagem estática |
| Mantine | 9.0.0 | Biblioteca de componentes (requer React 19.2+) |
| Zustand | 4.x | State management |
| @mantine/dropzone | 9.0.0 | Drag-drop monitores |
| react-router | 7.x | Roteamento de páginas |
| i18next + react-i18next | 23.x / 14.x | Internacionalização (en padrão + pt-BR; arquitetura extensível) |

### Dependências Externas

- **[VB-Audio Cable](https://vb-audio.com/Cable/)**: driver de áudio virtual gratuito (donationware). Necessário para criar os dispositivos virtuais que o Winx usa como ponte. Instalação manual seguindo guia integrado no app.
- **Windows 10 1903+** ou **Windows 11**: para suporte completo a WASAPI loopback e APIs modernas.
- **VC++ Redistributable 2015–2022**: dependência transitiva de drivers nativos.

### Identificadores do bundle

- **Bundle ID** (Tauri / MSI): `br.com.winxkvm.app`
- **Executável**: `winx-kvm.exe`
- **Service mDNS**: `_winx-kvm._tcp.local.`
- **Pasta de config**: `%APPDATA%\Winx-KVM\`
- **Service name no Credential Manager**: `Winx-KVM`

### Internacionalização (i18n)

- **Idioma padrão**: inglês (`en`).
- **Traduções incluídas no bundle**: `en`, `pt-BR`.
- **Arquitetura**: `react-i18next` com namespaces por feature; arquivos JSON em `ui/src/i18n/locales/<lang>/<namespace>.json`.
- **Auto-detect**: na primeira execução, lê locale do SO via `@tauri-apps/plugin-os` e seleciona se houver tradução; senão usa `en`. Usuário pode trocar em Settings.
- **Strings nunca hard-coded** em componentes — sempre via `t('key')`.

---

## Arquitetura

### Visão Geral

```
┌─────────────────────────────────────────┐         ┌─────────────────────────────────────────┐
│                   PC A                  │         │                   PC B                  │
│  ┌───────────────────────────────────┐  │         │  ┌───────────────────────────────────┐  │
│  │            UI (React)             │  │         │  │            UI (React)             │  │
│  │  Mantine + Zustand + Tauri events │  │         │  │  Mantine + Zustand + Tauri events │  │
│  └────────────────┬──────────────────┘  │         │  └────────────────┬──────────────────┘  │
│                   │ invoke / emit       │         │                   │ invoke / emit       │
│  ┌────────────────▼──────────────────┐  │         │  ┌────────────────▼──────────────────┐  │
│  │      Application (use cases)      │  │         │  │      Application (use cases)      │  │
│  │       Event bus (broadcast)       │  │         │  │       Event bus (broadcast)       │  │
│  └────────────────┬──────────────────┘  │         │  └────────────────┬──────────────────┘  │
│                   │                     │         │                   │                     │
│  ┌────────────────▼──────────────────┐  │         │  ┌────────────────▼──────────────────┐  │
│  │           Domain (puro)           │  │         │  │           Domain (puro)           │  │
│  │  Identity · Discovery · Pairing   │  │         │  │  Identity · Discovery · Pairing   │  │
│  │  Transport · Input · Media · Data │  │         │  │  Transport · Input · Media · Data │  │
│  └────────────────┬──────────────────┘  │         │  └────────────────┬──────────────────┘  │
│                   │                     │         │                   │                     │
│  ┌────────────────▼──────────────────┐  │         │  ┌────────────────▼──────────────────┐  │
│  │       Infrastructure (impl)       │  │  QUIC   │  │       Infrastructure (impl)       │  │
│  │  Win32 hooks · cpal · arboard     │◄─┼─────────┼─►│  Win32 hooks · cpal · arboard     │  │
│  │  mdns-sd · quinn · keyring · TOML │  │  mDNS   │  │  mdns-sd · quinn · keyring · TOML │  │
│  └───────────────────────────────────┘  │         │  └───────────────────────────────────┘  │
│                                         │         │                                         │
│   [Monitor 1] [Monitor 2]               │         │              [Monitor 1]                │
└─────────────────────────────────────────┘         └─────────────────────────────────────────┘
```

### Camadas (Arquitetura Hexagonal + DDD)

1. **Domain** (`winx-domain`): entidades, value objects, agregados e domain events. Zero I/O. Zero dependências externas além de `serde` e tipos primitivos.
2. **Application** (`winx-application`): casos de uso (commands/queries), orquestração, event bus interno, traits de ports (interfaces que o domínio precisa do mundo externo).
3. **Infrastructure** (`winx-infra`): adapters concretos que implementam as ports — chamadas Win32, network sockets, filesystem, drivers de áudio.
4. **Presentation** (`winx-kvm` binary + `ui/`): Tauri commands e events, React frontend.

**Regra de dependência**: setas apontam para dentro. Domain não conhece Application; Application não conhece Infrastructure; Infrastructure implementa traits de Application.

### Comunicação Entre Contexts

Usamos **event bus interno** via `tokio::sync::broadcast`. Cada bounded context expõe:

- **Comandos** (entrada): funções públicas que aceitam DTOs e retornam `Result`.
- **Eventos de domínio** (saída): variantes de um enum `DomainEvent` publicadas no bus.

Outros contexts assinam o bus e reagem aos eventos que lhes interessam. Isso desacopla os contexts e facilita testes (mockar o bus).

Exemplo de fluxo de pareamento:

```
1. UI invoca command `start_pairing(peer_id)`
2. Pairing publica `DomainEvent::PairingInitiated { session_id, pin }`
3. UI assina, mostra PIN na tela
4. Outro lado: usuário digita PIN, UI invoca `submit_pin(session_id, pin)`
5. Pairing valida, executa key exchange via Transport
6. Pairing publica `DomainEvent::PeerTrusted { peer_id, public_key }`
7. Identity assina, persiste o peer na lista de confiança via IdentityStore port
8. Transport assina, eleva a conexão de "handshake" para "established"
9. UI assina e atualiza estado para "Conectado"
```

---

## Modelo de Domínio (DDD)

São **7 bounded contexts**, cada um com agregado raiz claro e linguagem ubíqua bem definida.

### 1. Identity

**Responsabilidade**: identidade criptográfica do dispositivo local e lista de peers confiáveis.

**Agregados / entidades**:
- `Device` (raiz): id, username, par de chaves Ed25519, data de criação.
- `TrustedPeer`: id remoto, public key, username, data do pareamento, último visto.

**Value objects**: `DeviceId` (UUID), `PublicKey` (32 bytes), `Fingerprint` (hash SHA-256 truncado para exibição).

**Eventos**: `DeviceCreated`, `PeerTrusted`, `PeerForgotten`, `UsernameChanged`.

**Linguagem ubíqua**: *device* (este PC), *peer* (PC remoto), *trust* (relação persistente após pareamento), *fingerprint* (representação humana da chave pública).

### 2. Discovery

**Responsabilidade**: anunciar este device na LAN e manter lista de peers descobertos (não necessariamente confiáveis).

**Agregados**: `DiscoveryRegistry` (in-memory): mapa `PeerId → DiscoveredPeer`.

**Value objects**: `DiscoveredPeer { id, username, addresses: Vec<SocketAddr>, last_seen }`.

**Eventos**: `PeerAppeared`, `PeerDisappeared`, `PeerUpdated`.

**Linguagem ubíqua**: *announce* (anunciar via mDNS), *discover* (encontrar peer anunciado), *service* (`_winx-kvm._tcp.local.`).

### 3. Pairing

**Responsabilidade**: estabelecer confiança entre dois devices via PIN + troca de chaves.

**Agregados**: `PairingSession` (raiz): id, role (initiator/responder), peer_id, pin, state (`Requested → PinShown → PinSubmitted → Verified → Completed | Failed`), expires_at.

**Value objects**: `Pin` (6 dígitos numéricos), `PairingNonce`, `KeyExchange` (X25519 ephemeral keys).

**Eventos**: `PairingRequested`, `PairingPinReady`, `PairingPinSubmitted`, `PairingCompleted`, `PairingFailed`.

**Invariantes**:
- PIN expira em 90 segundos.
- Apenas uma `PairingSession` ativa por peer.
- Após 3 tentativas erradas, a sessão é cancelada.

### 4. Transport

**Responsabilidade**: gerenciar conexões QUIC ativas com peers confiáveis, multiplexar streams por tipo de dado.

**Agregados**: `PeerConnection` (raiz): peer_id, endpoint QUIC, streams (`Control`, `Input`, `Audio`, `Data`), state (`Connecting → Established → Disconnected`), métricas (RTT, perda).

**Value objects**: `StreamKind`, `ConnectionStats`, `EndpointAddress`.

**Eventos**: `ConnectionEstablished`, `ConnectionLost`, `StreamOpened`, `StreamClosed`, `StatsUpdated`.

**Streams**:
- **Control** (bidirecional, confiável): heartbeat, focus switch, monitor layout changes.
- **Input** (unidirecional, baixa latência): mouse/keyboard events do controlador para o controlado.
- **Audio** (unidirecional, datagram-like): chunks Opus.
- **Data** (bidirecional, confiável, por transferência): clipboard payload, arquivos.

### 5. InputControl

**Responsabilidade**: capturar input local, decidir para quem mandar (foco), injetar input vindo do peer, gerenciar layout de monitores e hotkeys.

**Agregados**:
- `FocusState` (raiz): qual `MachineId` está com o foco do mouse/teclado.
- `MonitorLayout` (raiz): lista de `MonitorPlacement`s no espaço virtual unificado.

**Value objects**:
- `Monitor { id, machine_id, resolution, physical_size, position_in_layout }`.
- `MousePosition { x, y, relative_to: MonitorId }`.
- `InputEvent` enum: `MouseMove`, `MouseClick`, `MouseScroll`, `KeyDown`, `KeyUp`.
- `Hotkey { modifiers, key, action: HotkeyAction }`.

**Eventos**: `FocusSwitched`, `MonitorAdded`, `MonitorRemoved`, `LayoutUpdated`, `HotkeyTriggered`, `InputBlocked`.

**Regras**:
- Quando foco está local, input é deixado passar e capturado por outras apps.
- Quando foco está em peer, low-level hook **engole** o input (`HC_ACTION` retorna 1) e envia via QUIC.
- Cursor durante controle remoto fica preso com `ClipCursor` numa janela invisível centrada.
- `Ctrl+Alt+Home` força foco para local.
- `Scroll Lock` ativa "lock mode" — borda não troca de foco.

### 6. Media

**Responsabilidade**: capturar áudio do VB-Cable, codificar Opus, transmitir; receber e reproduzir com jitter buffer.

**Agregados**: `AudioSession` (raiz): direção (`Outgoing` ou `Incoming`), peer_id, codec config, state.

**Value objects**: `AudioFrame { sample_rate, channels, samples }`, `OpusPacket`, `JitterBufferConfig`.

**Eventos**: `AudioSessionStarted`, `AudioSessionStopped`, `VbCableMissing`, `AudioDeviceChanged`.

**Pipeline outgoing**:
```
VB-Cable Output (Windows) → cpal capture → Opus encode (10ms frames) → QUIC Audio stream
```

**Pipeline incoming**:
```
QUIC Audio stream → Opus decode → Jitter buffer (20-60ms) → cpal output → VB-Cable Input
```

### 7. DataExchange

**Responsabilidade**: sincronização de clipboard e transferência de arquivos.

**Sub-agregados**:
- `ClipboardState`: último conteúdo conhecido em cada peer (hash do conteúdo para deduplicação).
- `FileTransfer`: id, direção, file metadata, progresso, hash esperado, state (`Pending → Streaming → Verifying → Completed | Failed`).

**Value objects**:
- `ClipboardPayload` enum: `Text(String)`, `Image { format, data }`, `Rtf(String)`, `FileList(Vec<PathBuf>)`.
- `FileChunk { transfer_id, offset, data }`.
- `ContentHash` (SHA-256).

**Eventos**: `ClipboardChanged`, `ClipboardReceived`, `FileTransferStarted`, `FileTransferProgress`, `FileTransferCompleted`, `FileTransferFailed`.

**Anti-loop**: cada clipboard sync carrega um `origin_peer_id`; recebedor não retransmite. Hash do conteúdo evita aplicar o mesmo payload duas vezes.

**Integração com Explorer (Ctrl+C/V de arquivos)**:
- Ao copiar arquivos no Explorer, Windows coloca `CF_HDROP` no clipboard local.
- Winx detecta, lê os paths, calcula tamanho total e hash.
- Envia apenas o **manifesto** (lista de paths + tamanhos + transfer_id) para o peer.
- Peer registra `IDataObject` virtual no clipboard com `CFSTR_FILEGROUPDESCRIPTOR` e `CFSTR_FILECONTENTS`.
- Ao colar, Windows chama o `IDataObject` virtual; Winx faz pull dos dados via QUIC stream sob demanda.

---

## Estrutura de Pastas

```
Winx-KVM/
├── README.md                        # Este arquivo
├── LICENSE
├── CHANGELOG.md
├── Cargo.toml                       # Workspace raiz
├── .gitignore
├── .editorconfig
│
├── crates/                          # Backend Rust (workspace)
│   ├── winx-kvm/                    # Binary Tauri (shell)
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   ├── build.rs
│   │   ├── icons/
│   │   └── src/
│   │       ├── main.rs              # Entry; configura Tauri + injeta DI
│   │       ├── app_state.rs         # State compartilhado com Tauri
│   │       ├── commands/            # #[tauri::command] handlers
│   │       │   ├── mod.rs
│   │       │   ├── identity.rs      # get_device_info, set_username
│   │       │   ├── discovery.rs     # list_discovered_peers
│   │       │   ├── pairing.rs       # start_pairing, submit_pin, cancel
│   │       │   ├── layout.rs        # get_layout, update_layout
│   │       │   ├── settings.rs      # get_settings, update_settings
│   │       │   └── transfers.rs     # list_transfers, pause, resume
│   │       └── events/              # Forward DomainEvent → emit_all
│   │           ├── mod.rs
│   │           └── forwarder.rs
│   │
│   ├── winx-domain/                 # Camada de domínio (puro)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── shared/              # Tipos atravessam contexts
│   │       │   ├── mod.rs
│   │       │   ├── ids.rs           # DeviceId, PeerId, SessionId
│   │       │   ├── machine.rs       # MachineId, MachineInfo
│   │       │   └── events.rs        # enum DomainEvent (união dos eventos)
│   │       ├── identity/
│   │       │   ├── mod.rs
│   │       │   ├── device.rs
│   │       │   ├── peer.rs
│   │       │   ├── keys.rs
│   │       │   └── events.rs
│   │       ├── discovery/
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs
│   │       │   └── events.rs
│   │       ├── pairing/
│   │       │   ├── mod.rs
│   │       │   ├── session.rs
│   │       │   ├── pin.rs
│   │       │   ├── key_exchange.rs
│   │       │   └── events.rs
│   │       ├── transport/
│   │       │   ├── mod.rs
│   │       │   ├── connection.rs
│   │       │   ├── stream_kind.rs
│   │       │   ├── stats.rs
│   │       │   └── events.rs
│   │       ├── input_control/
│   │       │   ├── mod.rs
│   │       │   ├── focus.rs
│   │       │   ├── monitor.rs
│   │       │   ├── layout.rs
│   │       │   ├── hotkey.rs
│   │       │   ├── input_event.rs
│   │       │   └── events.rs
│   │       ├── media/
│   │       │   ├── mod.rs
│   │       │   ├── session.rs
│   │       │   ├── codec.rs
│   │       │   ├── jitter_buffer.rs
│   │       │   └── events.rs
│   │       └── data_exchange/
│   │           ├── mod.rs
│   │           ├── clipboard.rs
│   │           ├── file_transfer.rs
│   │           ├── content_hash.rs
│   │           └── events.rs
│   │
│   ├── winx-application/            # Casos de uso + ports
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── bus/
│   │       │   ├── mod.rs
│   │       │   └── event_bus.rs     # tokio::broadcast wrapper
│   │       ├── ports/               # Traits que infra implementa
│   │       │   ├── mod.rs
│   │       │   ├── identity_store.rs
│   │       │   ├── secret_store.rs  # Credential Manager
│   │       │   ├── discovery.rs
│   │       │   ├── transport.rs
│   │       │   ├── input_backend.rs
│   │       │   ├── monitor_backend.rs
│   │       │   ├── audio_backend.rs
│   │       │   ├── clipboard_backend.rs
│   │       │   └── filesystem.rs
│   │       └── use_cases/
│   │           ├── identity/
│   │           │   ├── ensure_device.rs
│   │           │   └── set_username.rs
│   │           ├── discovery/
│   │           │   ├── start_announcing.rs
│   │           │   └── listen_peers.rs
│   │           ├── pairing/
│   │           │   ├── initiate.rs
│   │           │   ├── accept.rs
│   │           │   └── complete.rs
│   │           ├── transport/
│   │           │   ├── connect.rs
│   │           │   └── disconnect.rs
│   │           ├── input_control/
│   │           │   ├── capture_loop.rs
│   │           │   ├── inject_loop.rs
│   │           │   ├── switch_focus.rs
│   │           │   ├── update_layout.rs
│   │           │   └── apply_hotkey.rs
│   │           ├── media/
│   │           │   ├── start_audio.rs
│   │           │   └── stop_audio.rs
│   │           └── data_exchange/
│   │               ├── sync_clipboard.rs
│   │               ├── send_file.rs
│   │               └── receive_file.rs
│   │
│   ├── winx-infra/                  # Adapters concretos (impl de ports)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── identity_store_toml.rs
│   │       ├── secret_store_keyring.rs
│   │       ├── discovery_mdns.rs
│   │       ├── transport_quic.rs
│   │       ├── input_win32.rs
│   │       ├── monitor_win32.rs
│   │       ├── audio_cpal.rs
│   │       ├── clipboard_arboard.rs
│   │       ├── clipboard_files_win32.rs   # IDataObject virtual
│   │       └── filesystem_std.rs
│   │
│   └── winx-protocol/               # Wire format (compartilhado)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── version.rs
│           ├── handshake.rs         # ClientHello, ServerHello
│           ├── control.rs           # FocusSwitch, Heartbeat, LayoutSync
│           ├── input.rs             # InputPayload
│           ├── audio.rs             # AudioPayload (Opus packets)
│           ├── clipboard.rs         # ClipboardPayload
│           └── file.rs              # FileManifest, FileChunk
│
├── ui/                              # Frontend React
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── public/
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── theme.ts                 # Mantine theme
│       ├── routes/
│       │   ├── HomePage.tsx         # Lista de peers + status
│       │   ├── PairingPage.tsx      # Fluxo de PIN
│       │   ├── LayoutPage.tsx       # Drag-drop de monitores
│       │   ├── TransfersPage.tsx    # Histórico/progresso
│       │   ├── SettingsPage.tsx
│       │   └── AboutPage.tsx
│       ├── features/                # Espelha bounded contexts
│       │   ├── identity/
│       │   ├── discovery/
│       │   ├── pairing/
│       │   ├── monitor-layout/
│       │   ├── clipboard/
│       │   ├── transfers/
│       │   └── settings/
│       ├── components/              # Reutilizáveis (Card, PeerAvatar, etc)
│       ├── stores/                  # Zustand
│       │   ├── useDeviceStore.ts
│       │   ├── usePeersStore.ts
│       │   ├── usePairingStore.ts
│       │   ├── useLayoutStore.ts
│       │   ├── useTransfersStore.ts
│       │   └── useSettingsStore.ts
│       ├── ipc/
│       │   ├── commands.ts          # Wrappers tipados de `invoke`
│       │   └── events.ts            # Wrappers tipados de `listen`
│       ├── i18n/
│       │   ├── index.ts             # config i18next + detector
│       │   └── locales/
│       │       ├── en/              # idioma padrão (fonte da verdade)
│       │       │   ├── common.json
│       │       │   ├── pairing.json
│       │       │   ├── layout.json
│       │       │   └── settings.json
│       │       └── pt-BR/
│       │           ├── common.json
│       │           ├── pairing.json
│       │           ├── layout.json
│       │           └── settings.json
│       └── hooks/
│
├── docs/
│   ├── PLANNING.md                  # Roadmap detalhado e prioridades
│   ├── ARCHITECTURE.md              # Diagramas mais profundos
│   ├── PROTOCOL.md                  # Spec do wire format
│   ├── SECURITY.md                  # Threat model
│   ├── adr/                         # Architecture Decision Records
│   │   ├── 0001-rust-tauri-react.md
│   │   ├── 0002-p2p-topology.md
│   │   ├── 0003-quic-transport.md
│   │   ├── 0004-mdns-discovery.md
│   │   ├── 0005-ed25519-pin-pairing.md
│   │   ├── 0006-vb-audio-cable.md
│   │   ├── 0007-win32-hooks-input.md
│   │   ├── 0008-7-bounded-contexts.md
│   │   ├── 0009-event-bus-broadcast.md
│   │   └── 0010-toml-credentialmanager.md
│   └── images/
│
├── installer/
│   └── wix/                         # MSI via cargo-wix
│
└── scripts/
    ├── setup.ps1                    # Bootstrap dev env (rustup, node, etc)
    └── check-vb-cable.ps1           # Detecta VB-Cable instalado
```

---

## Protocolo de Comunicação

### Camada física

QUIC sobre UDP/443 (configurável). Uma conexão por peer, multiplexada em **streams tipados**:

| Stream | Direção | Reliability | Conteúdo |
|---|---|---|---|
| `Control` | Bidirecional | Confiável (stream) | Handshake, heartbeat, focus switch, layout sync |
| `Input` | Unidirecional | Datagram ou stream | Mouse/keyboard events do controlador |
| `Audio` | Unidirecional | Datagram | Opus packets (perda tolerada) |
| `Data` | Bidirecional, **um por transferência** | Confiável | Clipboard payload, file chunks |

### Wire format

Todas as mensagens são `bincode 2` com schema versionado:

```rust
// crates/winx-protocol/src/lib.rs
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
pub struct Frame {
    pub version: u16,
    pub payload: Payload,
}

#[derive(Serialize, Deserialize)]
pub enum Payload {
    Hello(handshake::ClientHello),
    Welcome(handshake::ServerHello),
    Control(control::ControlMessage),
    Input(input::InputPayload),
    Audio(audio::AudioPayload),
    Clipboard(clipboard::ClipboardPayload),
    FileManifest(file::FileManifest),
    FileChunk(file::FileChunk),
    Heartbeat,
}
```

### Handshake (resumido)

1. **TLS 1.3** já estabelecido pelo QUIC, usando certificado auto-assinado derivado da Ed25519 do device.
2. **ClientHello**: peer_id do device A + protocol version.
3. **ServerHello**: peer_id do device B + verificação de trust (B procura A em sua lista de `TrustedPeer`).
4. Se não confia: rejeita com `HandshakeRejected`; cliente abre fluxo de pareamento.
5. Se confia: ambos abrem streams `Control`, `Input`, `Audio` lazy (criados quando necessários).

---

## Modelo de Segurança

### Princípios

- **End-to-end encryption** sempre (TLS 1.3 do QUIC).
- **Trust on first pairing**: depois do PIN, é confiança persistente. Sem CA, sem servidor central.
- **Sem cloud**: nada sai da LAN.
- **Zero trust de rede**: mesmo na mesma LAN, qualquer peer não-pareado é rejeitado.

### Threat model

| Ameaça | Mitigação |
|---|---|
| Atacante na LAN tenta parear | PIN de 6 dígitos, expira em 90s, máx 3 tentativas |
| MITM no pareamento | Display do fingerprint (SHA-256 truncado) na UI dos dois lados — usuário compara |
| Roubo do PC pareado | Revogação manual via UI ("Esquecer este dispositivo") |
| Replay de input | Sequence numbers + timestamps nas mensagens; janela de aceitação curta |
| Vazamento de chave privada | Armazenada via DPAPI no Credential Manager (vinculada ao usuário Windows) |
| Clipboard com senha auto-sincronizado | Heurística para detectar saída de password managers (futura); por enquanto: documentar |

### Pareamento — passo a passo

1. PC A clica "Conectar" no card do PC B descoberto.
2. PC A gera `pairing_nonce_a` e mostra PIN aleatório de 6 dígitos.
3. PC A envia `PairingRequest { peer_id_a, ephemeral_x25519_pub_a, pin_commitment }` a B.
4. PC B exibe notificação "PC A quer conectar — digite o PIN exibido na tela do PC A".
5. Usuário digita PIN no PC B.
6. PC B envia `PairingResponse { ephemeral_x25519_pub_b, pin_hash }`.
7. PC A verifica PIN, deriva chave compartilhada (X25519) e assina troca com Ed25519 (long-term).
8. Trocam fingerprints visualmente confirmadas (opcional, recomendado).
9. Ambos salvam o peer como `TrustedPeer` com a public key Ed25519 do outro.

---

## Configuração e Persistência

### Localização

- **Config legível**: `%APPDATA%\Winx-KVM\config.toml`
- **Lista de peers**: `%APPDATA%\Winx-KVM\peers.toml`
- **Layout salvo**: `%APPDATA%\Winx-KVM\layouts\<peer_id>.toml`
- **Logs**: `%APPDATA%\Winx-KVM\logs\winx-YYYYMMDD.log` (rotação diária)
- **Chave privada Ed25519**: Windows Credential Manager (DPAPI), service name `Winx-KVM`, account `device_private_key`
- **Inbox de arquivos**: `%USERPROFILE%\Winx Inbox\<peer_name>\`

### Exemplo `config.toml`

```toml
[device]
username = "kingdaswinx"
display_name = "Desktop João"

[network]
quic_port = 47291
mdns_service = "_winx-kvm._tcp.local."

[hotkeys]
panic_focus = "ctrl+alt+home"
lock_focus = "scroll_lock"
toggle_clipboard_sync = "ctrl+alt+shift+c"

[clipboard]
auto_sync = true
sync_text = true
sync_images = true
sync_rtf = true
max_payload_mb = 20

[audio]
codec = "opus"
sample_rate = 48000
channels = 2
frame_ms = 10
jitter_buffer = "adaptive"

[ui]
theme = "dark"
language = "en"   # auto-detectado na primeira execução; falls back para "en"
```

### Exemplo `peers.toml`

```toml
[[peer]]
id = "8f3a-2c4b-..."
username = "kingdaswinx"
display_name = "Notebook João"
public_key_ed25519 = "..."
paired_at = "2026-05-15T14:32:11-03:00"
last_seen = "2026-05-15T18:21:03-03:00"
```

---

## Roadmap

| Versão | Escopo | Prazo estimado |
|---|---|---|
| **v0.1 (MVP)** | Pareamento + mouse/teclado + clipboard texto + hotkeys | 4–6 semanas |
| **v0.2** | Layout drag-drop + clipboard imagem/RTF + tray + tema | +3 semanas |
| **v0.3** | Transferência de arquivos via Ctrl+C/V | +4 semanas |
| **v0.4** | Áudio (VB-Cable + Opus + jitter) | +4 semanas |
| **v1.0** | Installer MSI + auto-update + polimento | +2 semanas |
| **v2.0+** | USB-C, multi-OS, mesh > 2 peers, driver áudio próprio | sem prazo |

**Total até v1.0: ~17–19 semanas** (assumindo dedicação parcial).

### Critérios de saída de cada fase

**v0.1 → v0.2**: dois PCs pareados arrastam mouse entre si por uma hora sem desconexão; clipboard de texto chega em < 200ms.

**v0.2 → v0.3**: drag-drop posiciona monitores corretamente e o cursor cruza nas bordas físicas certas; cores e fontes consistentes em tema dark/light.

**v0.3 → v0.4**: arquivo de 1GB transferido por Ctrl+C/V com hash íntegro, com pausa/retomada funcionando.

**v0.4 → v1.0**: áudio Spotify+chamada Discord rodando simultâneo sem glitch perceptível em uso normal por 30 minutos.

---

## Setup de Desenvolvimento

### Requisitos

- Windows 10 (1903+) ou Windows 11
- Rust stable >= 1.75 (`rustup default stable`)
- Node.js >= 20 (`winget install OpenJS.NodeJS.LTS`)
- pnpm >= 9 (`corepack enable && corepack prepare pnpm@latest --activate`)
- Visual Studio Build Tools 2022 com workload "Desktop C++" (para crates nativas)
- WebView2 Runtime (geralmente já presente em Win11)
- Tauri CLI (instalado via cargo)
- VB-Audio Cable (apenas para testar áudio): https://vb-audio.com/Cable/

### Bootstrap — Primeira execução

```powershell
# Clonar o repositório
git clone https://github.com/<seu-user>/Winx-KVM.git
cd Winx-KVM

# 1. Instalar Tauri CLI globalmente
cargo install tauri-cli

# 2. Setup Rust e Node.js
cargo fetch
cd ui
pnpm install
cd ..
```

### Desenvolvimento — Rodar a aplicação

```powershell
# A partir da raiz do projeto (onde está Cargo.toml)
cargo tauri dev
```

Isso abre a aplicação com **hot reload** automático tanto no frontend (React) quanto no backend (Rust). Qualquer mudança em código Rust dispara recompilação; mudanças no React recarregam a UI instantaneamente.

### Build de produção

```powershell
# A partir da raiz
cargo tauri build
# Artefatos em target/release/bundle/msi/
```

Gera o instalador MSI em `target/release/bundle/msi/`.

### Checagens antes de commit

```powershell
# Formatar código Rust
cargo fmt --all

# Lint (Clippy) — Zero warnings
cargo clippy --all-targets -- -D warnings

# Rodar testes Rust
cargo test --workspace

# TypeScript strict mode
cd ui
pnpm tsc --noEmit
pnpm lint
cd ..
```

### Padrões de código

- **Rust**: `cargo fmt` + `cargo clippy --all-targets -- -D warnings`. Convenção snake_case.
- **TypeScript**: ESLint + Prettier (config em `ui/`). camelCase.
- **Commits**: Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`).
- **Branches**: `main` é estável; trabalho em `feature/<nome>` ou `fix/<nome>`.

### Testes

- **Unit**: cada bounded context tem `#[cfg(test)]` adjacente. Sem I/O — usa mocks de ports.
- **Integration**: `tests/` em `winx-application/` orquestra dois peers in-process.
- **E2E** (futuro): scripts PowerShell que iniciam o app em duas VMs e validam fluxos.

---

## Decisões Arquiteturais (ADRs)

ADRs vivem em `docs/adr/`. Resumo:

| # | Decisão | Motivo |
|---|---|---|
| 0001 | Rust + Tauri + React | Performance nativa + UI rica + binário pequeno |
| 0002 | Topologia P2P pura, papéis dinâmicos | Controlar de qualquer PC sem reconfigurar |
| 0003 | QUIC (quinn) com streams multiplexados | Latência baixa + criptografia nativa + multiplex |
| 0004 | mDNS para descoberta | Zero-config, padrão da indústria |
| 0005 | Ed25519 + PIN 6 dígitos | Trust persistente, MITM-resistant, UX OK |
| 0006 | VB-Audio Cable como driver virtual | Único caminho viável sem escrever driver |
| 0007 | Win32 hooks diretos via `windows-rs` | Latência mínima, controle total |
| 0008 | 7 bounded contexts (DDD) | Modularidade sem boilerplate excessivo |
| 0009 | Event bus via tokio::broadcast | Desacoplamento + testabilidade |
| 0010 | TOML + Credential Manager | Legível + chaves seguras via DPAPI |

---

## Glossário

- **Device**: o PC local (este).
- **Peer**: PC remoto descoberto ou pareado.
- **Trusted Peer**: peer com pareamento persistido — confia sem PIN.
- **Pairing Session**: processo temporário de troca de PIN + chaves.
- **Focus**: qual peer está recebendo input de mouse/teclado neste momento.
- **Layout**: arranjo virtual dos monitores físicos de todos os peers.
- **Stream**: canal lógico dentro da conexão QUIC, dedicado a um tipo de dado.
- **Bounded context**: limite do modelo de domínio com linguagem ubíqua própria.
- **Port**: interface (trait Rust) que o domínio precisa do mundo externo.
- **Adapter**: implementação concreta de uma port — vive em `winx-infra`.
- **Event bus**: canal interno de eventos do domínio entre contexts (não rede).
- **VB-Cable**: driver de áudio virtual da VB-Audio que cria devices "CABLE Input" e "CABLE Output".

---

## Licença

MIT — veja [LICENSE](LICENSE).

## Autor

João Vitor Souza Moreira — kingdaswinxbr@proton.me
