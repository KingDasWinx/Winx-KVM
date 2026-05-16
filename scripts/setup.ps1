# scripts/setup.ps1
# Bootstrap do ambiente de desenvolvimento do Winx-KVM.

$ErrorActionPreference = 'Stop'

Write-Host '=== Winx-KVM dev bootstrap ===' -ForegroundColor Cyan

# --- Rust toolchain
Write-Host '`n[1/4] Verificando Rust...' -ForegroundColor Yellow
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Host 'Rust nao encontrado. Instale via https://rustup.rs' -ForegroundColor Red
    exit 1
}
rustc --version
rustup component add rustfmt clippy

# --- Node + pnpm
Write-Host '`n[2/4] Verificando Node e pnpm...' -ForegroundColor Yellow
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host 'Node nao encontrado. Instale Node 20+ (winget install OpenJS.NodeJS.LTS)' -ForegroundColor Red
    exit 1
}
node --version

if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    Write-Host 'Habilitando pnpm via corepack...' -ForegroundColor DarkGray
    corepack enable
    corepack prepare pnpm@latest --activate
}
pnpm --version

# --- Tauri CLI
Write-Host '`n[3/4] Verificando Tauri CLI...' -ForegroundColor Yellow
if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
    Write-Host 'Instalando tauri-cli 2.11.1...' -ForegroundColor DarkGray
    cargo install tauri-cli --version 2.11.1 --locked
} else {
    cargo tauri --version
}

# --- Instala deps
Write-Host '`n[4/4] Instalando dependencias do UI...' -ForegroundColor Yellow
Push-Location ui
try {
    pnpm install
} finally {
    Pop-Location
}

Write-Host '`n[OK] Setup concluido.' -ForegroundColor Green
Write-Host 'Para rodar em dev:' -ForegroundColor White
Write-Host '  cd crates/winx-kvm' -ForegroundColor DarkGray
Write-Host '  cargo tauri dev' -ForegroundColor DarkGray
Write-Host '`nAviso: gere os icones antes de `cargo tauri build`:' -ForegroundColor Yellow
Write-Host '  cargo tauri icon path/to/source-1024.png' -ForegroundColor DarkGray
