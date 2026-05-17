# Tutorial — Debugar mDNS Discovery com Wireshark

## Objetivo
Confirmar se dois PCs conseguem se **enxergar via mDNS** na rede, ou se o problema é firewall/rede vs código.

---

## Instalação

### Windows 10/11

1. **Download**: https://www.wireshark.org/download/
2. **Instalar**: Wireshark + Npcap (ask during install)
3. **Run as Admin**: (necessário para capturar pacotes)

---

## Setup — Antes de Começar

### **PC A (Notebook WiFi)** + **PC B (Desktop Ethernet)**

1. Abra **dois terminais** (um em cada PC)
2. **Rode o app em ambos** (deixe em background):
   ```powershell
   cd C:\Users\seu-usuario\Documents\GitHub\Winx-KVM
   cargo tauri dev
   ```
3. Deixe rodando por ~30 segundos (app anuncia, começa a buscar)

---

## Passo 1 — Abrir Wireshark

1. **Abra Wireshark como Admin** (run as administrator)
2. **Selecione a interface correta**:
   - **PC A (Notebook)**: selecione **Wi-Fi**
   - **PC B (Desktop)**: selecione **Ethernet**
   
   (Se não souber qual é, veja na lista qual tem atividade/luz verde)

3. Clique no ícone de **play azul** para iniciar captura

---

## Passo 2 — Filtrar apenas tráfego mDNS

Na caixa **Filter** no topo do Wireshark, digite:

```
mdns
```

Clique **Enter** ou pressione **Apply**.

**Isto vai mostrar APENAS pacotes mDNS**, filtrando todo o resto.

---

## Passo 3 — Esperar pelos Pacotes

**Deixe a captura rodando por ~15 segundos** enquanto o app funciona.

Você deve ver na lista:

```
No.  Time        Source        Destination   Protocol  Length  Info
1    0.123456    192.168.1.10  224.0.0.251   MDNS      XXX     Standard query 0x.... PTR _services._dns-sd._udp.local
2    0.234567    192.168.1.20  224.0.0.251   MDNS      XXX     Standard query 0x.... PTR _services._dns-sd._udp.local
3    1.345678    192.168.1.10  224.0.0.251   MDNS      XXX     Standard query 0x.... PTR _winx-kvm._tcp.local
...
```

---

## Passo 4 — Interpretar os Resultados

### ✅ Cenário Bom — Descoberta Funcionando

Você vê:

1. **Query do PC A**: `Standard query ... _winx-kvm._tcp.local`
   ```
   Source: 192.168.1.10 (PC A)
   Destination: 224.0.0.251 (multicast)
   Info: Standard query 0x.... PTR _winx-kvm._tcp.local
   ```

2. **Resposta do PC B**: `Standard query response 0x....`
   ```
   Source: 192.168.1.20 (PC B)
   Destination: 224.0.0.251 (multicast)  [ou unicast para PC A]
   Info: 0 answers, 1 authority, 0 additional
          ...winx-{uuid}._winx-kvm._tcp.local...
   ```

3. **PC B Anuncia**: `Standard query response 0x....`
   ```
   ...ServiceInfo com:
      - A record: 192.168.1.20
      - SRV record: winx-{uuid}._winx-kvm._tcp.local port 7878
      - TXT record: peer_id=..., username=..., fingerprint=...
   ```

**Se vir tudo isto → mDNS funciona! Problema é no código/mdns-sd parsing.**

---

### 🔴 Cenário Ruim #1 — PC A não consegue chegar a PC B

Você vê:

- **Query do PC A**: `Standard query ... _winx-kvm._tcp.local` ✅
- **Resposta do PC B**: ❌ **NUNCA APARECE**

**Diagnóstico**: 
- PC B não está recebendo a query (firewall bloqueia multicast 224.0.0.251:5353)
- **OU** PC B não consegue enviar resposta de volta

**Ação**:
```powershell
# No PC B, confirme que firewall permite mDNS
Get-NetFirewallRule -DisplayName "Winx-KVM*" | Get-NetFirewallPortFilter | Select-Object LocalPort, Protocol, Direction

# Esperado: LocalPort 5353, Protocol UDP, Direction Inbound
```

---

### 🔴 Cenário Ruim #2 — Ambos enviam query, mas nenhum responde

Você vê:

- **Query do PC A**: `Standard query ... _winx-kvm._tcp.local` ✅
- **Query do PC B**: `Standard query ... _winx-kvm._tcp.local` ✅
- **Resposta do A**: ❌ Nunca aparece
- **Resposta do B**: ❌ Nunca aparece

**Diagnóstico**: 
- Apps estão **anunciando** (você vê queries)
- Mas **não conseguem responder** (mdns-sd recebe query mas falha ao processar)
- **OU** resposta é enviada mas com problema (SRV/TXT inválido)

**Ação**: Expandir o Wireshark filter para ver **tudo**, não só mDNS sucesso:

```
dns
```

Isto mostra ALL DNS/mDNS, inclusive malformações.

---

## Passo 5 — Expandir Pacotes para Ver Detalhes

1. **Clique em um pacote** na lista para selecioná-lo
2. **Expanda a seção "Multicast DNS"** na árvore (clique no "+")
3. Você vai ver:

```
Multicast DNS
  Transaction ID: 0x0000
  Flags: 0x0000
  Questions: 1
    Name: _winx-kvm._tcp.local
    Type: PTR (12)
    Class: IN (1)
  Answer RRs: 0
  Authority RRs: 0
  Additional RRs: 0
```

---

## Passo 6 — Procurar por Respostas Específicas

Se vir **"Answer RRs: 0"** em uma resposta, significa que PC respondeu mas **SEM dados**.

Procure por um pacote que mostre:

```
Answer RRs: 1
  Name: winx-{UUID}._winx-kvm._tcp.local
  Type: PTR (12)
  Class: IN (1)
  TTL: 4500
  Data length: XX
  PTR: winx-{UUID}._winx-kvm._tcp.local
```

Se não vir isto, o problema é que **mdns-sd não consegue responder corretamente**.

---

## Passo 7 — Procurar por SRV e TXT Records

Expanda ainda mais:

```
Service Record (SRV) Record
  Name: winx-{UUID}._winx-kvm._tcp.local
  Type: SRV (33)
  Priority: 0
  Weight: 0
  Port: 7878
  Target: winx-{UUID}.local
```

E depois:

```
Text (TXT) Record
  Name: winx-{UUID}._winx-kvm._tcp.local
  Type: TXT (16)
  Text strings: 3
    peer_id=...
    username=...
    fingerprint=...
```

Se vir tudo isto, **mDNS announce está funcionando**.

---

## 🎯 Checklist de Diagnóstico

| Item | Esperado | Cenário | Ação |
|---|---|---|---|
| **Query mDNS aparece** | SIM | Sim → mDNS query funciona | Próximo passo |
| **Query aparece de ambos PCs** | Sim | Sim → ambos conseguem buscar | Próximo passo |
| **Resposta aparece** | Sim | Não → firewall está bloqueando resposta | Abrir firewall UDP 5353 resposta |
| **SRV record na resposta** | Sim | Não → mdns-sd não consegue serializar | Problema em mdns-sd |
| **TXT record com peer_id** | Sim | Não → dados corrompidos | Problema em ServiceInfo::new() |
| **Port 7878 no SRV** | 7878 | Outro → porta errada em config | Verificar WINX_KVM_PORT |

---

## 🔍 Leitura Avançada — Filtros Úteis

### Ver APENAS queries (não respostas)

```
mdns && mdns.flags.response == 0
```

### Ver APENAS respostas

```
mdns && mdns.flags.response == 1
```

### Ver tráfego de um IP específico

```
mdns && ip.src == 192.168.1.10
```

### Ver tráfego multicast

```
ip.dst == 224.0.0.251
```

### Ver tudo EXCETO mDNS

```
!mdns
```

---

## 🚨 Problemas Comuns

### "Wireshark não captura nada"
- Confirme que está rodando **como Admin**
- Confirme que a **interface correta** foi selecionada (W iFi/Ethernet)
- Tente **restart Wireshark**

### "Vejo muitos pacotes mDNS mas não _winx-kvm._tcp.local"
- Procure pelo serviço certo:
  ```
  mdns.mdns.name contains "winx"
  ```
- Se não achar nada, app pode não estar anunciando corretamente

### "Vejo queries mas respostas têm comprimento 0"
- PC respondeu mas **sem dados**
- Problema em ServiceInfo ou na serialização
- Verifique logs do app para erros em `register()`

---

## Exemplo Real — O Que Você Vai Ver

**PC A (192.168.1.10) rodando, PC B (192.168.1.20) rodando:**

```
No.  Time      Source        Dest          Protocol  Info
1    0.001     192.168.1.10  224.0.0.251   MDNS      Query _services._dns-sd._udp.local
2    0.045     192.168.1.20  224.0.0.251   MDNS      Response _services._dns-sd._udp.local, _winx-kvm._tcp.local
3    0.102     192.168.1.10  224.0.0.251   MDNS      Query _winx-kvm._tcp.local
4    0.156     192.168.1.20  224.0.0.251   MDNS      Response winx-AAAAA._winx-kvm._tcp.local, port 7878, TXT peer_id=...
5    0.201     192.168.1.20  224.0.0.251   MDNS      Query _services._dns-sd._udp.local
6    0.245     192.168.1.10  224.0.0.251   MDNS      Response _services._dns-sd._udp.local, _winx-kvm._tcp.local
7    0.302     192.168.1.20  224.0.0.251   MDNS      Query _winx-kvm._tcp.local
8    0.356     192.168.1.10  224.0.0.251   MDNS      Response winx-BBBBB._winx-kvm._tcp.local, port 7878, TXT peer_id=...
```

**Se vir isto → Discovery funciona! Problema é app parsing.**

---

## 🎬 Próximas Ações Após Wireshark

1. **Se vir respostas completas com SRV/TXT**: 
   - Problema é `mdns-sd` ou código não conseguindo processar eventos
   - **Voltar para code review + downgrade para 0.17.x**

2. **Se vir queries mas SEM respostas**:
   - Problema é **firewall ou rede**
   - **Verificar regras Winx-KVM + habilitar multicast**

3. **Se não vir NENHUM pacote mDNS**:
   - App pode não estar iniciando discovery
   - **Verificar logs procurando `[BROWSE THREAD] iniciado`**

---

## Comando Rápido — Verificar Conectividade mDNS

```powershell
# Windows — testar se consegue resolver via mDNS (pode não funcionar em todos Windows)
nslookup -type=PTR _services._dns-sd._udp.local 224.0.0.251

# Se não funcionar, tentar com ping multicast (não usual em Windows)
Test-NetConnection -ComputerName 224.0.0.251 -Port 5353 -TraceRoute
```

---

## Resumo

1. **Abra Wireshark como Admin**
2. **Selecione Wi-Fi (PC A) ou Ethernet (PC B)**
3. **Filter: `mdns`**
4. **Deixe capturando 15-30 segundos**
5. **Procure por**:
   - Query `_winx-kvm._tcp.local` ✅
   - Response com SRV + TXT ✅
   - Port 7878 no SRV ✅
   - `peer_id=...` no TXT ✅

**Se vir tudo → mDNS OK, problema é code.**
**Se faltar algo → problema é firewall/rede.**
