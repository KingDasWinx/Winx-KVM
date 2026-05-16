/**
 * Wrappers tipados em torno de `listen()` do Tauri.
 *
 * Todos os eventos do backend chegam num único canal `winx://event` com um
 * payload `{ kind: string }`. Conforme bounded contexts forem implementados,
 * adicionar discriminated unions aqui.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type WinxEvent =
  | { kind: 'placeholder' }
  // Sprint 1 — Identity:
  // | { kind: 'device_created'; payload: { device_id: string } }
  // Sprint 2 — Discovery:
  // | { kind: 'peer_appeared'; payload: { peer_id: string; username: string } }
  // ...
  ;

export function onWinxEvent(handler: (event: WinxEvent) => void): Promise<UnlistenFn> {
  return listen<WinxEvent>('winx://event', (e) => handler(e.payload));
}
