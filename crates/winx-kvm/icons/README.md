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

## Status

⚠️ Placeholders ausentes. `cargo tauri dev` pode emitir aviso e `cargo tauri build` **vai falhar** até que ícones reais sejam gerados.

Para o MVP, basta um ícone simples — a primeira versão pode ser feita rapidamente
em qualquer editor (sugiro um W estilizado em cor azul `#0078D6`).
