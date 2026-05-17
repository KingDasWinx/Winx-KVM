# Script para rodar o app com logging detalhado de mDNS
# Uso: .\run-with-debug-logs.ps1

Write-Host "=== Winx-KVM Debug Mode ===" -ForegroundColor Cyan
Write-Host "Rodando com logs detalhados de mDNS Discovery..." -ForegroundColor Yellow
Write-Host ""
Write-Host "Logs serão salvos em: %APPDATA%\br.com.winxkvm.app\logs\winx-kvm.log" -ForegroundColor Green
Write-Host ""

# Set environment variables para logging detalhado
$env:RUST_LOG = "winx_infra::discovery_mdns=trace,winx_application::use_cases::discovery=debug,info"
$env:WINX_LOG = "winx=debug,info"

Write-Host "[DEBUG] Iniciando app com:" -ForegroundColor Cyan
Write-Host "  RUST_LOG=$env:RUST_LOG" -ForegroundColor Gray
Write-Host "  WINX_LOG=$env:WINX_LOG" -ForegroundColor Gray
Write-Host ""

# Limpar logs anteriores (opcional — comentar se quiser acumular)
# $logDir = "$env:APPDATA\br.com.winxkvm.app\logs"
# if (Test-Path $logDir) {
#     Remove-Item "$logDir\winx-kvm.log" -Force -ErrorAction SilentlyContinue
#     Write-Host "[INFO] Logs anteriores removidos" -ForegroundColor Green
# }

Write-Host "Iniciando cargo tauri dev..." -ForegroundColor Yellow
Write-Host ""

# Rodar o app
cargo tauri dev

Write-Host ""
Write-Host "=== App encerrado ===" -ForegroundColor Cyan
Write-Host "Logs detalhados salvos em: %APPDATA%\br.com.winxkvm.app\logs\winx-kvm.log" -ForegroundColor Green
