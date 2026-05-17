# Ícones do Winx-KVM

Esta pasta deve conter os ícones referenciados em [`tauri.conf.json`](../tauri.conf.json):

- `32x32.png`
- `128x128.png`
- `128x128@2x.png`
- `icon.icns` (macOS — futuro)
- `icon.ico` (Windows)

## Gerar a partir de uma fonte 1024×1024

```powershell
# A partir da raiz do repo, com `cargo tauri` instalado:
cargo tauri icon caminho/para/winx-kvm-source-1024.png
```

O comando gera todos os formatos automaticamente.

## Fonte

- `app-icon-source.png` — arte original (pode não ser quadrada)
- `app-icon-1024.png` — crop 1024×1024 usado por `cargo tauri icon`

## Regenerar

```powershell
cd crates/winx-kvm
cargo tauri icon icons/app-icon-1024.png
```
