# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status

**Implementação ativa.** Sprints 1–9 do MVP (v0.1) concluídos exceto F9.3 (smoke test em VM limpa).
Sprint W1–W4 da feature **Workspaces** documentado em [Workspace-TODO.md](Workspace-TODO.md), aguardando início.
Leia [README.md](README.md) (visão, arquitetura, decisões) e [docs/PLANNING.md](docs/PLANNING.md) (backlog).

**Usuário se comunica em português brasileiro** (mantenha docs e respostas em pt-BR), mas a **UI do app é em inglês como padrão**, com pt-BR como tradução completa (i18n via `react-i18next`).

## Stack travada

Não troque de versão sem pedir. Confirme no Context7 antes de adicionar dependência nova.

- **Backend**: Rust stable, Tauri **2.11.1**, tokio, quinn (QUIC), rustls, mdns-sd, windows-rs, cpal, opus, arboard, keyring, serde+toml, tracing
- **Frontend**: React **19.2**, Vite **8.x**, TypeScript, Mantine **9.0.0** (último indexado no Context7 compatível com React 19.2+), Zustand, react-router 7, **i18next + react-i18next**
- **Externa**: VB-Audio Cable (instalação manual pelo usuário; necessário só para áudio)
- **Plataforma**: Windows 10 1903+ / Windows 11 (sem suporte cross-OS na v1)

## Identificadores

- **Bundle ID**: `br.com.winxkvm.app`
- **Executável**: `winx-kvm.exe`
- **Service mDNS**: `_winx-kvm._tcp.local.`
- **Config dir**: `%APPDATA%\Winx-KVM\`
- **Credential Manager service**: `Winx-KVM`

## Arquitetura (resumo operacional)

Workspace Cargo com 5 crates + frontend em `ui/`. Padrão **DDD + Hexagonal**.

```
crates/winx-kvm/         Binary Tauri (shell, commands, events)
crates/winx-domain/      Domain puro (entidades, value objects, eventos)
crates/winx-application/ Use cases + ports (traits) + event bus
crates/winx-infra/       Adapters concretos (impl das ports)
crates/winx-protocol/    Wire format (serde+bincode, compartilhado entre peers)
ui/                      React + Vite (build vai para ../crates/winx-kvm/dist)
```

**Regra de dependência inviolável**: `domain` ← `application` ← `infra` / `winx-kvm`. Domain não pode importar nada concreto (nem tokio). Application define traits; Infra implementa.

**7 bounded contexts** dentro de `winx-domain`: `identity`, `discovery`, `pairing`, `transport`, `input_control`, `media`, `data_exchange`. Cada um com submódulos `events.rs` + agregados. Veja README §"Modelo de Domínio" para responsabilidades.

**Comunicação entre contexts**: event bus via `tokio::sync::broadcast` em `winx-application/src/bus/`. Contexts publicam `DomainEvent` e assinam o que importa. Nunca chame outro context diretamente — sempre via evento ou via port.

**Streams QUIC tipados**: `Control` (handshake, focus, heartbeat) | `Input` (mouse/keyboard) | `Audio` (Opus datagrams) | `Data` (clipboard, files, 1 stream por transfer).

**Estado Tauri**: `AppState` global só carrega o `EventBus`. Cada bounded context tem sua struct própria (`IdentityState`, `DiscoveryState`, `PairingState`, etc.) registrada via `app.manage()` no `setup()`. Isso mantém acoplamento zero — commands de um context não veem os ports de outro.

**Commands são thin**: handlers em `winx-kvm/src/commands/` apenas chamam o use case correspondente e mapeiam o resultado para DTO. Erros viajam como `DomainErrorCode` (string estável) pra ser traduzido no frontend via i18n.

## Comandos de desenvolvimento

Quando o workspace estiver scaffold (sprint 1 ainda não rodou):

```powershell
# Setup inicial
cargo fetch
cd ui; pnpm install; cd ..

# Dev (abre app com hot reload no front e watch no back)
cargo tauri dev

# Build de produção (gera MSI em target/release/bundle/)
cargo tauri build

# Checagens (rode antes de commit)
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace

# Rodar um teste único
cargo test -p winx-domain pairing::session::tests::pin_expires_after_90s

# Frontend lint/typecheck
cd ui; pnpm lint; pnpm tsc --noEmit
```

Shell padrão do usuário é **PowerShell** no Windows. Em scripts, use sintaxe PowerShell (`$env:VAR`, `;` para chain, não `&&`). Bash também está disponível via tool, mas prefira PS para qualquer coisa que o usuário possa rodar manualmente.

## Convenções específicas deste projeto

- **Persistência**: config legível em `%APPDATA%\Winx-KVM\*.toml`; chave privada Ed25519 **só** no Windows Credential Manager via `keyring` (DPAPI). Nunca grave segredos em TOML/JSON.
- **Versionar wire format**: `winx-protocol::PROTOCOL_VERSION` muda sempre que `Payload` muda de forma incompatível. Mensagens carregam `version: u16` no header.
- **Anti-loop de clipboard**: toda `ClipboardPayload` carrega `origin_peer_id`. Receptor verifica e não retransmite. Use hash do conteúdo para deduplicar.
- **Trust persistente**: após pareamento, peer fica em `peers.toml` com public key Ed25519. Não exigir PIN de novo. Revogação manual via UI ("Esquecer dispositivo").
- **Foco mutado por um único Mutex**: toda mudança de `FocusState` passa por `tokio::sync::Mutex` em `input_control` para evitar race entre captura local e injeção remota.
- **Hotkeys reservadas**: `Ctrl+Alt+Home` (pânico, força foco local) e `Scroll Lock` (lock no PC atual). Configuráveis em `config.toml` mas defaults invioláveis na UI.
- **i18n obrigatório**: nenhuma string de UI hard-coded. Sempre `const { t } = useTranslation('namespace'); t('key')`. Namespaces atuais: `common`, `settings`, `lab` (Workspace-TODO.md adicionará `workspace`). Adicione a chave em `ui/src/i18n/locales/en/<namespace>.json` (fonte da verdade) e em `pt-BR/<namespace>.json`. CI deve falhar se houver chave faltando em alguma locale (lint custom).
- **Lab page** (`ui/src/pages/LabPage.tsx` + `crates/winx-kvm/src/commands/lab.rs`): playground de diagnóstico (connectivity suite, keyboard mirror, input debug). Use ele pra testar mudanças no transport/input antes de exercitar pelo fluxo real.
- **Commits**: Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`).
- **Branches**: trunk-based; `main` sempre verde; `feature/<nome>` curta (< 1 semana).

## Pontos de atenção

- **Win32 hooks são frágeis**: `LowLevelMouseProc`/`KeyboardProc` precisam de message loop ativa na mesma thread. Coloque numa thread dedicada com `PeekMessage`. Sob carga (jogo rodando), o Windows pode descartar o hook se demorar > 300ms para processar — mantenha o handler enxuto e empurre o trabalho para uma channel.
- **ClipCursor é por-processo**: se você crashar com cursor clipped, o usuário perde o mouse. Sempre `restore` em `Drop` e em `Ctrl+C` handler.
- **QUIC + cert auto-assinado**: rustls não aceita por padrão. Use `ServerCertVerifier` custom que valida pela public key Ed25519 do peer pareado, não pelo cert.
- **VB-Audio Cable pode não estar instalado**: trate `cpal::Device` ausente como estado normal — exibe UI educativa em vez de panic.
- **Mantine 9 exige React 19.2+**: confirmado no changelog (mantine.dev/changelog/9-0-0). Não tente fazer Mantine 9 com React 18.

## Memória persistente

Memórias entre sessões ficam em `C:\Users\kingdaswinx\.claude\projects\c--Users-kingdaswinx-Documents-GitHub-Winx-KVM\memory\`. Veja `MEMORY.md` lá para o índice.
