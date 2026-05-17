# Script para analisar logs de mDNS após rodar o app
# Uso: .\analyze-mdns-logs.ps1

$logFile = "$env:APPDATA\br.com.winxkvm.app\logs\winx-kvm.log"

if (-not (Test-Path $logFile)) {
    Write-Host "❌ Log não encontrado em: $logFile" -ForegroundColor Red
    Write-Host "Certifique-se de ter rodado 'run-with-debug-logs.ps1' primeiro" -ForegroundColor Yellow
    exit 1
}

Write-Host "=== ANÁLISE DE LOGS mDNS ===" -ForegroundColor Cyan
Write-Host "Arquivo: $logFile" -ForegroundColor Gray
Write-Host ""

# Função para contar ocorrências
function Count-Lines {
    param($pattern, $label)
    $count = (Select-String -Path $logFile -Pattern $pattern -ErrorAction SilentlyContinue | Measure-Object).Count
    Write-Host "$label: $count" -ForegroundColor Green
}

Write-Host "📊 RESUMO DE EVENTOS:" -ForegroundColor Yellow
Write-Host ""

Write-Host "[MDNS INIT]" -ForegroundColor Cyan
Count-Lines "\[MDNS INIT\]" "  Logs de inicialização"
Write-Host ""

Write-Host "[MDNS ANNOUNCE]" -ForegroundColor Cyan
Count-Lines "\[MDNS ANNOUNCE\]" "  Logs de announce"
Count-Lines "\[MDNS ANNOUNCE\] sucesso" "  → Announces bem-sucedidos"
Count-Lines "\[MDNS ANNOUNCE\] falha" "  → Falhas"
Write-Host ""

Write-Host "[BROWSE THREAD]" -ForegroundColor Cyan
Count-Lines "\[BROWSE THREAD\]" "  Logs do browse thread"
$browseEvents = (Select-String -Path $logFile -Pattern "evento #" -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "  Eventos mDNS recebidos: $browseEvents" -ForegroundColor Green
Write-Host ""

Write-Host "[BROWSE] ServiceResolved:" -ForegroundColor Cyan
$resolved = (Select-String -Path $logFile -Pattern "\[BROWSE\] ServiceResolved" -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "  Peers encontrados: $resolved" -ForegroundColor Green
Write-Host ""

Write-Host "[BROWSE] ServiceRemoved:" -ForegroundColor Cyan
$removed = (Select-String -Path $logFile -Pattern "\[BROWSE\] ServiceRemoved" -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "  Peers removidos: $removed" -ForegroundColor Green
Write-Host ""

Write-Host "[BROWSE] SearchStarted:" -ForegroundColor Cyan
$started = (Select-String -Path $logFile -Pattern "\[BROWSE\] SearchStarted" -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "  Buscas iniciadas: $started" -ForegroundColor Green
Write-Host ""

Write-Host "---" -ForegroundColor Gray
Write-Host ""

Write-Host "⚠️  DIAGNÓSTICO:" -ForegroundColor Yellow
Write-Host ""

if ($browseEvents -eq 0) {
    Write-Host "🔴 CRÍTICO: Nenhum evento mDNS recebido!" -ForegroundColor Red
    Write-Host "   → Browse thread pode estar travado" -ForegroundColor Yellow
    Write-Host "   → mdns-sd daemon pode não estar funcionando" -ForegroundColor Yellow
} else {
    Write-Host "✅ Browse está recebendo eventos" -ForegroundColor Green
}

if ($resolved -eq 0) {
    Write-Host "🔴 CRÍTICO: Nenhum peer resolvido!" -ForegroundColor Red
    Write-Host "   → Ou browse não encontrou nenhum serviço anunciado" -ForegroundColor Yellow
    Write-Host "   → Ou parsing de ServiceResolved está falhando" -ForegroundColor Yellow
    Write-Host "   Verifique logs procurando por '[BROWSE] ServiceFound'" -ForegroundColor Yellow
} else {
    Write-Host "✅ Peers foram resolvidos: $resolved" -ForegroundColor Green
}

if ($started -eq 0) {
    Write-Host "⚠️  AVISO: SearchStarted nunca foi logado" -ForegroundColor Yellow
    Write-Host "   → Browse pode não ter sido inicializado corretamente" -ForegroundColor Yellow
} else {
    Write-Host "✅ Browse foi iniciado: $started vez(es)" -ForegroundColor Green
}

Write-Host ""
Write-Host "---" -ForegroundColor Gray
Write-Host ""
Write-Host "📋 EXEMPLOS DE LOGS:" -ForegroundColor Yellow
Write-Host ""

Write-Host "Últimas linhas sobre mDNS:" -ForegroundColor Cyan
Select-String -Path $logFile -Pattern "(\[MDNS|\[BROWSE)" | Select-Object -Last 30 | ForEach-Object {
    Write-Host $_.Line -ForegroundColor Gray
}

Write-Host ""
Write-Host "---" -ForegroundColor Gray
Write-Host ""
Write-Host "💡 PRÓXIMOS PASSOS:" -ForegroundColor Yellow
Write-Host ""
Write-Host "1. Se 'Nenhum evento mDNS recebido':" -ForegroundColor Cyan
Write-Host "   → Rode: Wireshark com filtro 'mdns' em ambos os PCs" -ForegroundColor Gray
Write-Host "   → Confirme se PC A envia query e PC B responde" -ForegroundColor Gray
Write-Host ""
Write-Host "2. Se 'Browse iniciado' mas 'Nenhum peer resolvido':" -ForegroundColor Cyan
Write-Host "   → Procure por '[BROWSE] ServiceFound' nos logs" -ForegroundColor Gray
Write-Host "   → Se existir, é problema no parsing de ServiceResolved" -ForegroundColor Gray
Write-Host ""
Write-Host "3. Envie o arquivo de log completo para análise:" -ForegroundColor Cyan
Write-Host "   → Caminho: $logFile" -ForegroundColor Gray
Write-Host ""
