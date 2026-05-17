# Winx-KVM — Planejamento de Implementação

Este documento complementa o [README](../README.md) com o **plano executável**: backlog do MVP, ordem de implementação, riscos e pontos abertos que ainda precisam de decisão antes ou durante a codificação.

---

## Sumário

- [Princípios de execução](#princípios-de-execução)
- [Backlog do MVP (v0.1)](#backlog-do-mvp-v01)
- [Ordem recomendada](#ordem-recomendada)
- [Estimativa por sprint](#estimativa-por-sprint)
- [Riscos e mitigações](#riscos-e-mitigações)
- [Pontos abertos](#pontos-abertos)
- [Métricas de qualidade](#métricas-de-qualidade)

---

## Princípios de execução

1. **Ponta-a-ponta antes de profundidade**: prefira fazer uma fatia vertical funcionar (UI → app → domain → infra) antes de polir cada camada.
2. **Domain primeiro, infra depois**: monte o domínio com tipos e testes unitários antes de plugar QUIC/Win32.
3. **Mocks de ports nos testes**: cada use-case testável sem rede.
4. **Commits pequenos, PRs por feature**: facilita reverter e revisar.
5. **Feature flags via Cargo features**: `audio`, `file-transfer`, `clipboard-images` ligadas separadamente.
6. **Trunk-based**: branch único `main` estável; `feature/*` curtas (< 1 semana).

---

## Backlog do MVP (v0.1)

### Épico 1 — Workspace e fundação

- [x] **F1.1** Criar workspace Cargo com 5 crates: `winx-kvm`, `winx-domain`, `winx-application`, `winx-infra`, `winx-protocol`
- [x] **F1.2** Configurar `tauri.conf.json` apontando `frontendDist` para `../../ui/dist`
- [x] **F1.3** Scaffold do React: `pnpm create vite ui --template react-ts` + Mantine + Zustand
- [x] **F1.4** Configurar `tracing` + `tracing-subscriber` com filtro por env var
- [x] **F1.5** Setup de CI básico (GitHub Actions): `cargo check`, `cargo clippy`, `cargo test`, `pnpm build`
- [x] **F1.6** Configurar `.editorconfig`, `rustfmt.toml`, `clippy.toml`, `.prettierrc`

### Épico 2 — Identity

- [x] **F2.1** `Device` entity com geração de keypair Ed25519 na primeira execução
- [x] **F2.2** `TrustedPeer` entity e operações `trust`, `forget`
- [x] **F2.3** Port `IdentityStore` + adapter TOML
- [x] **F2.4** Port `SecretStore` + adapter `keyring` (Credential Manager)
- [x] **F2.5** Use case `ensure_device()`: lê ou cria identidade
- [x] **F2.6** Tauri command `get_device_info()` + emit event `device-ready`
- [x] **F2.7** UI: tela inicial mostrando username + fingerprint do dispositivo

### Épico 3 — Discovery

- [x] **F3.1** Port `DiscoveryAdapter` com métodos `announce(device_info)` e `subscribe()` (Stream)
- [x] **F3.2** Adapter `mdns-sd`: serviço `_winx-kvm._tcp.local.`, TXT records com peer_id e username
- [x] **F3.3** `DiscoveryRegistry` em `winx-domain` mantém lista de peers vistos
- [x] **F3.4** Use case `start_announcing()` + loop `listen_peers()`
- [x] **F3.5** Eventos `PeerAppeared` / `PeerDisappeared` no bus
- [x] **F3.6** Tauri command `list_discovered_peers()` + event `peers-updated`
- [x] **F3.7** UI: cards de peers descobertos com avatar/nome/fingerprint

### Épico 4 — Pairing

- [x] **F4.1** `PairingSession` com state machine validada (testes de cada transição)
- [x] **F4.2** `Pin` value object: gera 6 dígitos cryptographically random
- [x] **F4.3** Lógica de key exchange X25519 efêmero
- [x] **F4.4** Mensagens de protocolo: `PairingRequest`, `PairingResponse`, `PairingConfirm` (UDP 7879, `winx-protocol::pairing`)
- [x] **F4.5** Use cases `initiate_pairing(peer_id)`, `submit_pin(session_id, pin)`, `cancel_pairing`
- [x] **F4.6** Expiração: timer 90s; rate limit 3 tentativas erradas
- [x] **F4.7** Persistência: ao completar, grava em `peers.toml`
- [x] **F4.8** Tauri commands + events
- [x] **F4.9** UI: modal de pareamento com PIN grande na tela do initiator; input no responder

### Épico 5 — Transport

- [x] **F5.1** Cert auto-assinado derivado da Ed25519 (necessário para QUIC/TLS)
- [x] **F5.2** Endpoint QUIC com `quinn`: server + client integrados
- [x] **F5.3** Mapping `peer_id → endpoint_address` mantido in-memory pelo Discovery
- [x] **F5.4** Estabelecimento de conexão após pareamento ou redescoberta
- [x] **F5.5** Streams nomeados: open `Control` no handshake, lazy `Input` quando focus muda
- [x] **F5.6** Heartbeat a cada 5s no Control; timeout de 15s força reconexão
- [x] **F5.7** Métricas: RTT, perda, throughput → event `stats-updated` (throttle 1Hz)
- [x] **F5.8** UI: indicador de status (Verde conectado / Amarelo reconectando / Vermelho offline)

### Épico 6 — InputControl básico

- [x] **F6.1** Port `InputBackend` com `start_capture(handler)`, `inject(event)`, `set_cursor_clipped(rect)`
- [x] **F6.2** Adapter Win32: `SetWindowsHookEx` com `WH_MOUSE_LL` + `WH_KEYBOARD_LL`
- [x] **F6.3** Adapter Win32: `SendInput` para injeção
- [x] **F6.4** Tradução de eventos Windows VK_CODE ↔ enum portável
- [x] **F6.5** Port `MonitorBackend` + adapter `EnumDisplayMonitors`
- [x] **F6.6** `MonitorLayout` simples (lado-a-lado, sem drag-drop por enquanto)
- [x] **F6.7** Lógica de focus switch: detect cursor crossing edge → emit `FocusSwitched`
- [x] **F6.8** Bloquear/desbloquear input local com `HC_ACTION` retornando 1
- [x] **F6.9** `ClipCursor` + janela transparente full-screen para prender cursor local
- [x] **F6.10** Wire: envia `InputPayload` no stream Input
- [x] **F6.11** Hotkey de pânico `Ctrl+Alt+Home` registrado via `RegisterHotKey`
- [x] **F6.12** Hotkey `Scroll Lock` como toggle de lock

### Épico 7 — Clipboard (texto)

- [x] **F7.1** Port `ClipboardBackend` com observer (clipboard change events)
- [x] **F7.2** Adapter `arboard` + polling 200ms (Windows não tem evento confiável)
- [x] **F7.3** Detecta texto, calcula hash, ignora se igual ao último
- [x] **F7.4** Wire: stream Data com `ClipboardPayload::Text`
- [x] **F7.5** Receptor: escreve no clipboard local via `arboard`, marca origin para evitar loop
- [x] **F7.6** Toggle UI: "Sincronizar clipboard automaticamente"

### Épico 8 — UI base

- [x] **F8.1** Theme Mantine custom (dark default) + tipografia
- [x] **F8.2** Layout principal: sidebar com navegação + área de conteúdo
- [x] **F8.3** Página Home: lista de peers, status, ações rápidas
- [x] **F8.4** Página Settings: username, hotkeys, toggles, **seletor de idioma**
- [x] **F8.5** Tray icon mínimo (mostrar/esconder janela, sair)
- [x] **F8.6** Notificações toast via Mantine
- [x] **F8.7** **i18n com `react-i18next` desde o início**:
  - Setup de `i18next` em `ui/src/i18n/index.ts` com `LanguageDetector` (Tauri OS plugin) + fallback `en`
  - Namespaces: `common`, `pairing`, `layout`, `settings` (um JSON por feature/locale)
  - Locales bundladas: `en` (fonte da verdade), `pt-BR` (tradução completa)
  - Hook `useTranslation` em **todos** os componentes — zero strings literais em JSX
  - Script `pnpm i18n:check` valida que todas as chaves de `en/` existem em `pt-BR/` (falha CI)
  - Mensagens de erro vindas do Rust também passam por i18n: o backend envia uma `error_code` (string estável) e a UI traduz

### Épico 9 — Empacotamento

- [x] **F9.1** Configurar `tauri build` para gerar MSI
- [x] **F9.2** Ícones (1024px) processados via `tauri icon`
- [ ] **F9.3** Smoke test em VM Windows limpa

---

## Ordem recomendada

```
Sprint 1 (Sem 1-2): Fundação + Identity
  → Épico 1 + Épico 2
  Critério: app abre, gera identidade, mostra fingerprint na tela

Sprint 2 (Sem 2-3): Discovery
  → Épico 3
  Critério: dois PCs na mesma LAN se enxergam

Sprint 3 (Sem 3-4): Pairing
  → Épico 4
  Critério: pareamento completo persistido em ambos os lados

Sprint 4 (Sem 4-5): Transport
  → Épico 5
  Critério: conexão QUIC estável, heartbeat funcionando, status no UI

Sprint 5 (Sem 5-6): Input
  → Épico 6
  Critério: mouse atravessa fronteira, teclado funciona no remoto, hotkeys OK

Sprint 6 (Sem 6): Clipboard + UI base + empacotamento
  → Épicos 7, 8, 9
  Critério: MSI instalável, fluxo completo end-to-end por 1 hora sem crash
```

---

## Estimativa por sprint

| Sprint | Esforço (h) | Risco | Notas |
|---|---|---|---|
| 1 | 30–40h | Baixo | Familiarização com Tauri 2.11.1 + Mantine 9.0.0 + setup i18n |
| 2 | 20–25h | Baixo | `mdns-sd` é bem documentado |
| 3 | 35–45h | **Alto** | Crypto é fácil de errar; testes precisam ser exaustivos |
| 4 | 40–50h | **Alto** | Quinn tem APIs novas; certificate handling |
| 5 | 45–60h | **Muito Alto** | Win32 hooks são notoriamente cheios de pegadinhas |
| 6 | 20–30h | Médio | UI polish; MSI signing pode dar trabalho |

**Total**: 190–250h ≈ 5–6 semanas em tempo integral, ou 10–16 semanas part-time.

---

## Riscos e mitigações

| # | Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|---|
| R1 | Win32 hooks atrasam ou perdem eventos sob carga | Alta | Alto | Benchmark cedo no sprint 5; fallback para RawInput se necessário |
| R2 | `ClipCursor` não funciona com algumas placas de vídeo Nvidia em multi-monitor | Média | Alto | Plan B: hide-cursor + janela tela cheia transparente que captura tudo |
| R3 | mDNS bloqueado em redes corporativas/restritas | Média | Médio | Adicionar fallback de broadcast UDP em v0.2 |
| R4 | Quinn tem breaking changes (versão pre-1.0) | Média | Médio | Pin versão exata, atualizar deliberadamente |
| R5 | VB-Cable não detecta no PC do usuário | Alta | Médio | UI explícita "Áudio desabilitado: VB-Cable não encontrado" com link de instalação |
| R6 | Certificado QUIC auto-assinado rejeitado pelo Windows Defender Firewall | Média | Alto | Instalar regra de firewall no instalador MSI |
| R7 | Latência de input > 20ms torna experiência ruim | Baixa | Alto | Medir p99 desde sprint 5; otimizar serialização se necessário |
| R8 | Race conditions entre captura de input e mudança de foco | Alta | Médio | Toda mutação de foco passa pelo mesmo `tokio::sync::Mutex` |
| R9 | Bug de loop infinito de clipboard sync (A→B→A→B) | Alta | Baixo | Toda payload carrega origin_peer_id; receptor não retransmite |
| R10 | ~~Mantine 9.2.1 não existir~~ | ~~Confirmada~~ | ~~Baixo~~ | **Resolvido**: travado em Mantine **9.0.0** (último indexado no Context7) |

---

## Pontos abertos

Itens que precisam de decisão antes ou durante a implementação:

### Antes do início

1. ~~**Confirmação da versão exata do Mantine**~~ — **Resolvido**: Mantine **9.0.0** (último indexado no Context7 compatível com React 19.2+).
2. ~~**Confirmação da versão do Tauri**~~ — **Resolvido**: Tauri **2.11.1** (lançado em maio/2026, confirmado pelo usuário).
3. ~~**Nome do executável e bundle ID**~~ — **Resolvido**: executável `winx-kvm.exe`, bundle ID `br.com.winxkvm.app`.
4. ~~**Localização inicial padrão**~~ — **Resolvido**: **`en` como idioma padrão**, com `pt-BR` traduzido completo desde a v0.1. Arquitetura `react-i18next` com namespaces por feature; auto-detect via `@tauri-apps/plugin-os`; suporte a mais idiomas é estender adicionando pasta em `ui/src/i18n/locales/<lang>/`.

### Durante o sprint 3 (Pairing)

5. **Comparação de fingerprints**: obrigatória ou opcional? Sugestão: opcional na v0.1, com toast educativo.
6. **Recuperação se um lado falhar no meio do pareamento**: timeout e retry automático?

### Durante o sprint 5 (Input)

7. **Como decidir qual é o monitor "fronteira" entre dois PCs no layout simples (sem drag-drop ainda)?** Sugestão v0.1: assume notebook à direita do desktop, sem configuração.
8. **Sensibilidade do mouse**: enviar deltas brutos ou aplicar aceleração? Sugestão: deltas brutos (preserva config de cada PC).
9. **Comportamento com múltiplos monitores no mesmo PC**: assume contínuo (cursor atravessa internamente como Windows faz).

### Durante o sprint 6 (Clipboard)

10. **Limite de tamanho de payload**: sugestão 20MB para imagens; rejeitar texto > 5MB com aviso.
11. **Polling vs hook do clipboard**: Windows não tem evento confiável; sugestão polling 200ms.

### Pós-MVP

12. **Telemetria opt-in**: o que coletar? Sugestão: versão, OS, contagem de pareamentos, sem PII.
13. **Auto-update**: usar Tauri Updater ou self-distributed? Sugestão: Tauri Updater quando tiver code signing.

---

## Métricas de qualidade

A barra para considerar uma feature "pronta":

- [ ] Testes unitários do domínio com cobertura > 80%
- [ ] Pelo menos um teste de integração por use case
- [ ] Latência p99 de input < 10ms em LAN cabeada
- [ ] Sem panic em 1h de uso normal
- [ ] Sem leak de memória > 5MB/h (monitorar com `dh.exe` ou Process Explorer)
- [ ] Build limpo: zero warnings clippy, zero erros TypeScript strict
- [ ] Smoke test em VM Windows limpa passa
- [ ] Documentação inline (`///` em funções públicas) atualizada

---

## Próximos passos imediatos

Após validar este planejamento:

1. Criar o workspace Cargo com os 5 crates vazios e o `Cargo.toml` raiz
2. Scaffolding do `ui/` com `pnpm create vite`
3. Validar `cargo tauri dev` abrindo janela vazia
4. Começar **F2.1** (geração de keypair Ed25519) — primeiro código de domínio real

---

**Última atualização**: 2026-05-16
