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

export interface DeviceInfo {
  id: string;
  username: string;
  /** Formato: "A3:4F:B2:1C:D9:E5:07:AB" */
  fingerprint: string;
}

export async function getDeviceInfo(): Promise<DeviceInfo> {
  return await invoke<DeviceInfo>('get_device_info');
}

export async function updateDeviceUsername(username: string): Promise<DeviceInfo> {
  return await invoke<DeviceInfo>('update_device_username', { username });
}

export interface DiscoveredPeer {
  id: string;
  username: string;
  /** Fingerprint Ed25519 do peer: "A3:4F:B2:1C:D9:E5:07:AB" */
  fingerprint: string;
  addresses: string[];
  /** Presente em `peers.toml` (pareamento concluído). */
  is_paired: boolean;
}

export async function listDiscoveredPeers(): Promise<DiscoveredPeer[]> {
  return await invoke<DiscoveredPeer[]>('list_discovered_peers');
}

export interface PairingInitiated {
  session_id: string;
  pin: string;
}

export async function initiatePairing(peerId: string, peerUsername: string): Promise<PairingInitiated> {
  return await invoke<PairingInitiated>('initiate_pairing', { peerId, peerUsername });
}

export async function acceptPairing(peerId: string, peerEphemeralPublicHex: string): Promise<PairingInitiated> {
  return await invoke<PairingInitiated>('accept_pairing', { peerId, peerEphemeralPublicHex });
}

export async function submitPin(sessionId: string, pin: string): Promise<void> {
  return await invoke<void>('submit_pin', { sessionId, pin });
}

export async function cancelPairing(sessionId: string): Promise<void> {
  return await invoke<void>('cancel_pairing', { sessionId });
}

export interface ConnectionStatsDto {
  rtt_ms: number;
  tx_bytes: number;
  rx_bytes: number;
  lost_packets: number;
}

export async function openConnection(peerId: string): Promise<void> {
  return await invoke<void>('open_connection', { peerId });
}

export async function disconnectPeer(peerId: string): Promise<void> {
  return await invoke<void>('disconnect_peer', { peerId });
}

export async function getConnectionStats(peerId: string): Promise<ConnectionStatsDto> {
  return await invoke<ConnectionStatsDto>('get_connection_stats', { peerId });
}

export interface ConnectionStateDto {
  peer_id: string;
  status: 'connecting' | 'connected' | 'reconnecting' | 'disconnected';
  rtt_ms?: number;
  tx_bytes?: number;
  rx_bytes?: number;
}

export async function listConnectionStates(): Promise<ConnectionStateDto[]> {
  return await invoke<ConnectionStateDto[]>('list_connection_states');
}

export interface FocusStateDto {
  /** `"local"` ou UUID do peer remoto com foco. */
  target: string;
  lock_mode: boolean;
}

export async function getFocusState(): Promise<FocusStateDto> {
  return await invoke<FocusStateDto>('get_focus_state');
}

export async function enableInputControl(peerId: string): Promise<void> {
  return await invoke<void>('enable_input_control', { peerId });
}

export async function getClipboardAutoSync(): Promise<boolean> {
  return await invoke<boolean>('get_clipboard_auto_sync');
}

export async function setClipboardAutoSync(enabled: boolean): Promise<void> {
  return await invoke<void>('set_clipboard_auto_sync', { enabled });
}

export async function enableClipboardSync(peerId: string): Promise<void> {
  return await invoke<void>('enable_clipboard_sync', { peerId });
}

export async function getFirewallStatus(): Promise<boolean> {
  return await invoke<boolean>('get_firewall_status');
}

export async function reconfigureFirewall(): Promise<void> {
  return await invoke<void>('reconfigure_firewall');
}

export async function exportDiagnostics(): Promise<string> {
  return await invoke<string>('export_diagnostics');
}

export interface NetworkInterface {
  name: string;
  ipv4: string | null;
}

export async function listNetworkInterfaces(): Promise<NetworkInterface[]> {
  return await invoke<NetworkInterface[]>('list_network_interfaces');
}

export async function getDiscoveryInterfaces(): Promise<string[]> {
  return await invoke<string[]>('get_discovery_interfaces');
}

export async function setDiscoveryInterfaces(interfaces: string[]): Promise<void> {
  return await invoke<void>('set_discovery_interfaces', { interfaces });
}
