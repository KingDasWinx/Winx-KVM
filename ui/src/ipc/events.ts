/**
 * Wrappers tipados em torno de `listen()` do Tauri.
 *
 * Todos os eventos do backend chegam num único canal `winx://event` com um
 * payload `{ kind: string }`. Conforme bounded contexts forem implementados,
 * adicionar discriminated unions aqui.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type WinxEvent =
  | { kind: 'device-created'; device_id: string; fingerprint: string }
  | { kind: 'peer-forgotten'; device_id: string }
  // Sprint 2 — Discovery
  | { kind: 'peers-updated'; peer_id?: string; peer_username?: string }
  // Sprint 3 — Pairing
  | { kind: 'pairing-incoming'; session_id: string; peer_id: string; peer_username: string }
  | { kind: 'pairing-completed'; session_id: string; peer_id: string; peer_username: string }
  | { kind: 'pairing-cancelled'; session_id: string; peer_id: string }
  | { kind: 'pairing-failed'; session_id: string; peer_id: string }
  // Sprint 4 — Transport
  | {
      kind: 'connection-established';
      peer_id: string;
      peer_username: string;
      is_inbound?: boolean;
      workspace_id?: string;
    }
  | { kind: 'connection-lost'; peer_id: string; reason?: string }
  | { kind: 'stats-updated'; peer_id: string; rtt_ms: number; tx_bytes: number; rx_bytes: number }
  // Sprint 5 — InputControl
  | { kind: 'focus-switched'; focus_from: string; focus_to: string }
  | { kind: 'hotkey-triggered'; hotkey_action: string }
  | { kind: 'input-blocked'; peer_id?: string; input_blocked: boolean }
  // Sprint 6 — Clipboard
  | { kind: 'clipboard-changed'; clipboard_hash?: string; clipboard_byte_len?: number }
  | { kind: 'clipboard-received'; peer_id?: string; clipboard_hash?: string }
  // Sprint W2 — Workspace
  | { kind: 'workspace-invite-incoming'; invite_id: string; workspace_id: string; workspace_name: string; peer_id: string; peer_username: string; fingerprint: string }
  | { kind: 'workspace-invite-accepted'; invite_id: string }
  | { kind: 'workspace-invite-rejected'; invite_id: string }
  | { kind: 'workspace-invite-expired'; invite_id: string }
  | {
      kind: 'workspaces-updated';
      workspace_id?: string;
      workspace_name?: string;
      new_version?: number;
      sync_from_remote?: boolean;
    }
  | { kind: 'workspace-marked-orphan'; workspace_id: string }
  | { kind: 'workspace-member-presence'; workspace_id: string; peer_id: string; is_online: boolean }
  | { kind: 'workspace-connected'; workspace_id: string }
  | { kind: 'workspace-disconnected'; workspace_id: string }
  | { kind: 'workspace-connection-conflict'; workspace_id: string; other_workspace_id: string }
  | {
      kind: 'workspace-global-cursor';
      workspace_id: string;
      x: number;
      y: number;
      seq?: number;
    }
  | { kind: 'unknown' };

export function onWinxEvent(handler: (event: WinxEvent) => void): Promise<UnlistenFn> {
  return listen<WinxEvent>('winx://event', (e) => handler(e.payload));
}
