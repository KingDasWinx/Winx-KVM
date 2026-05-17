# Debug mDNS Discovery — Teste Passo a Passo

## Status
Foram adicionados logs **ultra-detalhados** em `discovery_mdns.rs` para diagnosticar por que dois PCs não se enxergam via mDNS.

---

## Como Executar Teste

### **Passo 1: No PC A (Notebook WiFi)**

```powershell
cd C:\Users\kingdaswinx\Documents\GitHub\Winx-KVM
.\run-with-debug-logs.ps1
```

**Esperado**:
- Tauri dev server inicia
- Logs aparecem no console
- App abre em `http://localhost:5173`

**Deixe rodando** por ~30 segundos.

---

### **Passo 2: No PC B (Desktop Ethernet)**

Faça o mesmo:
```powershell
cd C:\Users\kingdaswinx\Documents\GitHub\Winx-KVM
.\run-with-debug-logs.ps1
```

**Deixe rodando** por ~30 segundos.

---

### **Passo 3: Analisar Logs**

Após rodarem, execute em **cada PC**:

```powershell
.\analyze-mdns-logs.ps1
```

Ele vai gerar um relatório mostrando:
- ✅ Quantos eventos mDNS foram recebidos
- ✅ Quantos peers foram resolvidos
- ✅ Se o browse foi iniciado
- 🔴 Diagnóstico automático (o que pode estar errado)

---

## Indicadores Críticos

Procure pelo output da análise:

### **🟢 Tudo OK** (esperado):
```
✅ Browse está recebendo eventos
✅ Peers foram resolvidos: 1
✅ Browse foi iniciado: 1 vez(es)
```
→ Se vir isto, o mDNS está funcionando. Problema é em outra camada.

### **🔴 Browse Travado** (crítico):
```
🔴 CRÍTICO: Nenhum evento mDNS recebido!
```
→ Browse thread pode estar:
  1. Travado/deadlock
  2. mdns-sd daemon não respondendo
  3. Problema na rede (firewall bloqueando 224.0.0.251:5353)

### **🔴 Nenhum Peer Resolvido** (crítico):
```
🔴 CRÍTICO: Nenhum peer resolvido!
```
→ Browse recebe eventos, mas:
  1. ServiceFound (antes de resolve) não está acontecendo
  2. ServiceResolved está falhando
  3. Parsing de UUID está quebrando

---

## Investigação Adicional (se logs não forem conclusivos)

### **Teste com Wireshark** (em ambos os PCs)

1. Abra Wireshark como Admin
2. Selecione interface (WiFi ou Ethernet)
3. Filtro: `mdns`
4. Deixe capturando enquanto apps rodando
5. Procure por:
   - Query multicast de PC A procurando `_winx-kvm._tcp.local.`
   - Resposta multicast de PC B com seu serviço

Se ver ambos → problema é **app**, não rede.
Se ver só um → problema é **firewall** ou **rede**.

### **Teste DNS Query Manual** (no PowerShell)

```powershell
# Consultar mDNS multicast (pode não funcionar em todos os Windows)
nslookup -type=PTR _services._dns-sd._udp.local 224.0.0.251

# Ou usar Resolve-DnsName (Windows 8+)
Resolve-DnsName -Name "_winx-kvm._tcp.local" -Type SRV
```

---

## Arquivos de Saída

Após rodar `run-with-debug-logs.ps1`:

**Logs completos**:
```
%APPDATA%\br.com.winxkvm.app\logs\winx-kvm.log
```

**Exemplos do que você vai ver** (extraído dos logs):

```
2026-05-17T13:11:40 INFO [MDNS INIT] criando ServiceDaemon...
2026-05-17T13:11:40 INFO [MDNS INIT] ServiceDaemon criado com sucesso
2026-05-17T13:11:40 INFO [MDNS INIT] mDNS daemon initialized (interface auto-detection ativo)
2026-05-17T13:11:40 INFO [MDNS ANNOUNCE] instance_name=winx-ef1b59e1-a726-4fdc-93c6-ada9b59f6493, host_name=winx-ef1b59e1-a726-4fdc-93c6-ada9b59f6493.local., port=7878
2026-05-17T13:11:40 INFO [MDNS ANNOUNCE] sucesso! peer_id=ef1b59e1-a726-4fdc-93c6-ada9b59f6493, port=7878
2026-05-17T13:11:40 INFO iniciando browse mDNS para tipo de serviço: _winx-kvm._tcp.local.
2026-05-17T13:11:40 INFO browse iniciado com sucesso, aguardando eventos...
2026-05-17T13:11:40 DEBUG [BROWSE THREAD] iniciado
2026-05-17T13:12:00 DEBUG [BROWSE] ServiceFound (antes de resolve): type=_winx-kvm._tcp.local., fullname=winx-abc123...._winx-kvm._tcp.local.
2026-05-17T13:12:00 DEBUG [BROWSE] ServiceResolved: fullname=winx-abc123...._winx-kvm._tcp.local., port=7878
2026-05-17T13:12:00 INFO [BROWSE] peer resolvido com sucesso
```

---

## Checklist de Teste

- [ ] Rodei `run-with-debug-logs.ps1` no PC A
- [ ] Rodei `run-with-debug-logs.ps1` no PC B
- [ ] Deixei ambos rodando por 30s
- [ ] Rodei `analyze-mdns-logs.ps1` em ambos
- [ ] Copiei saída da análise
- [ ] Verifiquei se há eventos mDNS
- [ ] Verifiquei se há peers resolvidos
- [ ] (Opcional) Rodei Wireshark em ambos

---

## Próximas Ações Baseadas em Resultado

### Se vir "✅ Peers foram resolvidos: 1 ou mais"
→ mDNS funciona! Problema pode ser:
  1. Eventos não chegam ao frontend (verificar WebSocket/Tauri event bus)
  2. UI não atualiza lista (verificar React component)
  3. Logs da aplicação (não só discovery)

### Se vir "🔴 Nenhum evento mDNS recebido"
→ Problema no mdns-sd ou firewall:
  1. Verifique regras de firewall com PowerShell:
     ```powershell
     Get-NetFirewallRule -DisplayName "Winx-KVM*"
     ```
  2. Teste manual com Wireshark
  3. Considere downgrade de `mdns-sd 0.19.1` → `0.18.x`

### Se vir "🔴 Browse iniciado mas nenhum peer resolvido"
→ Problema no parsing ou ServiceFound:
  1. Procure por `[BROWSE] ServiceFound` — existe?
  2. Se sim, problema é `ServiceFound` → `ServiceResolved`
  3. Se não, browse não vê nada (firwall issue)

---

## Commit das Mudanças

Já foi feito commit com os logs adicionados:

```
git log --oneline -1
# fix: add detailed mDNS discovery logging for debugging
```

Se quiser revert:
```powershell
git revert HEAD
```

---

## Dúvidas?

Se os logs não forem conclusivos, collect e envie:
1. Output de `analyze-mdns-logs.ps1` (ambos PCs)
2. Arquivo completo de logs: `%APPDATA%\br.com.winxkvm.app\logs\winx-kvm.log`
3. Output de `Get-NetFirewallRule -DisplayName "Winx-KVM*"`
4. (Opcional) Capture Wireshark (.pcapng) com mDNS filter

Com isso conseguimos identificar a causa-raiz rapidinho.
