# Correção KVM: hook panic, streams QUIC e panic hotkey

> **Status:** implementado em 2026-05-17.

**Goal:** Mouse cruza a borda, move no PC remoto, Ctrl+Alt+Home devolve o cursor ao monitor local, e disconnect libera o cursor em ambos os lados.

**Mudanças principais:**

1. `input_win32.rs` — `mouse_delta` com `wrapping_sub`, `SKIP_MOUSE_DELTA`, `reset_mouse_delta_baseline`, `warp_cursor` via `SetCursorPos`.
2. `transport.rs` — rejeita conexão entrante duplicada; `Connection.is_outbound` para papel QUIC.
3. `input_control.rs` — log do primeiro inject remoto; `panic_local` com warp na borda local; reset de baseline ao voltar foco local.
4. UI — hint `transport.single_connect_hint`; i18n en + pt-BR.
5. `docs/TESTE-ENTRE-PCS.md` — checklist single-connect e logs esperados.

**Teste manual:** apenas um PC clica Connect; ver `docs/TESTE-ENTRE-PCS.md` seção KVM.
