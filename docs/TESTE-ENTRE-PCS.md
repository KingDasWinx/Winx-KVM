# Testar Winx-KVM entre dois PCs (LAN)

Guia rápido para gerar o instalador/executável e validar discovery, pareamento e conexão entre duas máquinas Windows na mesma rede.

---

## 1. Gerar o build (no PC de desenvolvimento)

### Pré-requisitos (uma vez)

| Ferramenta | Uso |
|------------|-----|
| [Rust stable](https://rustup.rs/) | Compilar o backend |
| [Node 20+](https://nodejs.org/) + `pnpm` | Build do frontend |
| [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) | Runtime da UI (já vem no Win10/11 recente) |
| [WiX Toolset 3.x](https://wixtoolset.org/) | Gerar o `.msi` (só na máquina que faz `tauri build`) |

Instale o CLI do Tauri (se ainda não tiver):

```powershell
cargo install tauri-cli --version "^2.11"
```

### Comando de release

Na **raiz do repositório**:

```powershell
cargo tauri build
```

Isso executa `pnpm build` em `ui/` e compila o Rust em modo release. Ao terminar, os artefatos ficam em:

| Artefato | Caminho típico |
|----------|----------------|
| **Executável** (copiar direto) | `target\release\winx-kvm.exe` |
| **Instalador MSI** | `target\release\bundle\msi\Winx-KVM_0.0.1_x64_pt-BR.msi` (nome pode variar) |

Para teste rápido entre PCs você pode:

1. **MSI (recomendado)** — instala atalho, desinstalação limpa e costuma configurar permissões melhor.
2. **Só o `.exe`** — copie `winx-kvm.exe` para o outro PC; exige WebView2 instalado e você cria regras de firewall manualmente.

---

## 2. O que levar para o segundo PC

- O arquivo `.msi` **ou** `winx-kvm.exe`
- Os dois PCs na **mesma rede local** (mesmo Wi‑Fi ou switch Ethernet)
- Perfil de rede Windows definido como **Privada** (não Pública) nos dois

Dados do app após instalar:

- Config: `%APPDATA%\Winx-KVM\`
- Chave privada: Credential Manager (`Winx-KVM`)

---

## 3. Portas e firewall

O Winx-KVM usa:

| Protocolo | Porta / serviço | Direção | Função |
|-----------|-----------------|--------|--------|
| **UDP** | **5353** | Entrada (rede local) | mDNS — descoberta `_winx-kvm._tcp` |
| **UDP** | **7878** | Entrada (rede local) | QUIC — dados entre peers pareados |
| **UDP** | **7879** | Entrada (rede local) | Pareamento pré-confiança (PIN) |
| **UDP** | alta (efêmera) | Saída | Respostas QUIC / mDNS / pairing |

### Liberar no Windows Defender Firewall

Em **cada PC**, após a primeira execução (ou antes do teste):

1. `Win + R` → `wf.msc` → Enter  
2. **Regras de Entrada** → **Nova regra…**  
3. Tipo: **Programa** → caminho do executável, por exemplo:  
   `C:\Program Files\Winx-KVM\winx-kvm.exe`  
   (ou a pasta onde você colocou o `.exe` portátil)  
4. Ação: **Permitir a conexão**  
5. Perfil: marque **Privado** (e Domínio se usar)  
6. Nome: `Winx-KVM`

Repita para **Regras de Saída** se o firewall bloquear saída UDP (menos comum em rede privada).

**Alternativa rápida (teste):** quando o Windows pedir permissão na primeira abertura do app, marque redes **privadas** e aceite.

### Roteador

- Não é necessário abrir portas no roteador (sem port forwarding) — tudo é LAN.
- **AP isolation / “client isolation”** no Wi‑Fi deve estar **desligado**, senão os PCs não se veem.

---

## 4. Passo a passo no uso (dois PCs)

### PC A e PC B

1. Instale o MSI (ou execute `winx-kvm.exe`) em ambos.  
2. Abra o app — ícone na bandeja do Windows.  
3. Em **Settings**, defina um **nome de exibição** diferente em cada máquina (ex.: `Desktop` e `Notebook`).  
4. Na **Home**, aguarde a lista **“Devices on this network”** — o outro PC deve aparecer em alguns segundos.

### Pareamento (primeira vez)

1. **PC A (initiator):** clique **Pair** no card do peer — abre modal com **PIN de 6 dígitos** grande (não deve aparecer toast “Pairing request” neste PC).  
2. **PC B (responder):** toast **“Pairing request”** no canto inferior — digite o mesmo PIN exibido no PC A.  
3. Ambos mostram sucesso; `%APPDATA%\Winx-KVM\peers.toml` em cada máquina ganha o peer confiável.

Se o pedido aparecer no mesmo PC que clicou em Pair, o pareamento de rede não está funcionando (ver firewall UDP **7879**).

### Conectar

1. Clique **Connect** no peer pareado.  
2. Toast **“Connected”** quando o QUIC subir.  
3. Input remoto e clipboard passam a funcionar conforme os épicos já implementados (mover foco na borda do monitor, etc.).

### Atalhos úteis (padrão v0.1)

| Atalho | Ação |
|--------|------|
| `Ctrl+Alt+Home` | Pânico — força foco de volta a este PC |
| `Scroll Lock` | Trava o foco no PC atual (não troca na borda) |

Tray: clique esquerdo no ícone mostra/oculta a janela; menu **Show** / **Quit**.

---

## 5. Problemas comuns

| Sintoma | O que verificar |
|---------|-----------------|
| Peer não aparece na lista | Mesma rede? Firewall UDP 5353? AP isolation desligado? |
| Pair sem toast no outro PC | Firewall UDP **7879**? Mesma LAN? Rode setup de firewall (UAC) no app se disponível. |
| Toast “Pairing request” no PC que iniciou Pair | Bug de versão antiga — atualize o build. |
| Aparece mas Connect falha | Pareamento feito? `peers.toml` nos dois? Firewall UDP **7878**? |
| Nome antigo na rede | Em Settings, salve o username de novo (re-anuncia mDNS); ou reinicie o app nos dois |
| Tela branca ao abrir | Instale [WebView2 Runtime](https://go.microsoft.com/fwlink/p/?LinkId=2124703) |
| `tauri build` falha no MSI | Instale WiX 3.x e garanta `light.exe` / `candle.exe` no PATH |

### Logs (debug)

```powershell
$env:WINX_LOG = "debug"
.\winx-kvm.exe
```

(ou atalho com variável de ambiente definida)

---

## 6. Resumo dos identificadores

| Item | Valor |
|------|--------|
| Executável | `winx-kvm.exe` |
| Porta QUIC | **7878/UDP** |
| Porta pairing | **7879/UDP** |
| mDNS | `_winx-kvm._tcp.local.` |
| Bundle ID | `br.com.winxkvm.app` |

---

## 7. Próximo passo (F9.3)

Smoke test formal em VM Windows limpa (sem Rust/Node instalados) — apenas MSI + WebView2 + regras de firewall acima.
