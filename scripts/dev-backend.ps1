# Vite deve já estar rodando em http://localhost:5173 (scripts/dev-frontend.ps1)
$repoRoot = (Resolve-Path $PSScriptRoot\..).Path
$env:CARGO_TARGET_DIR = Join-Path $repoRoot "target"
Set-Location (Join-Path $repoRoot "crates\winx-kvm")
cargo watch -x "run --no-default-features"
