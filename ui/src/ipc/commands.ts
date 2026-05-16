/**
 * Wrappers tipados em torno de `invoke()` do Tauri.
 *
 * Convenção: cada command Rust tem um wrapper aqui que define o contrato
 * TypeScript. Erros do backend chegam como `{ code: string, message: string }`
 * — a UI deve traduzir `code` via i18n e nunca exibir `message`.
 */

import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
  name: string;
  version: string;
  protocol_version: number;
}

export interface DomainErrorPayload {
  code: string;
  message: string;
}

export async function getAppInfo(): Promise<AppInfo> {
  return await invoke<AppInfo>('app_info');
}

export async function ping(message: string): Promise<string> {
  return await invoke<string>('ping', { message });
}
