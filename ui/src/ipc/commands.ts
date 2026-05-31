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
  is_outbound: boolean;
  via_workspace_id?: string;
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

export interface ProbeResultDto {
  service: string;
  ok: boolean;
  latency_ms?: number;
  detail: string;
}

export interface LabProbeResultsDto {
  peer_id: string;
  probes: ProbeResultDto[];
  ran_at_ms: number;
}

export async function runConnectivitySuite(peerId: string): Promise<LabProbeResultsDto> {
  return await invoke<LabProbeResultsDto>('run_connectivity_suite', { peerId });
}

export interface KeyboardMirrorStatusDto {
  active: boolean;
  seconds_left: number;
  keys_sent: number;
  keys_hooked: number;
  keys_send_errors: number;
}

export interface InputDebugStatsDto {
  mirror_active: boolean;
  keys_sent: number;
  keys_hooked: number;
  keys_send_errors: number;
  remote_frames_received: number;
  remote_inject_ok: number;
  remote_inject_fail: number;
  mouse_frames_sent: number;
  input_enabled: boolean;
  has_input_tx: boolean;
}

export async function getInputDebugStats(): Promise<InputDebugStatsDto> {
  return await invoke<InputDebugStatsDto>('get_input_debug_stats');
}

export async function startKeyboardMirrorTest(
  peerId: string,
  durationSecs?: number,
): Promise<void> {
  return await invoke<void>('start_keyboard_mirror_test', {
    peerId,
    durationSecs,
  });
}

export async function getKeyboardMirrorStatus(): Promise<KeyboardMirrorStatusDto> {
  return await invoke<KeyboardMirrorStatusDto>('get_keyboard_mirror_status');
}

export async function sendTestClick(peerId: string): Promise<void> {
  return await invoke<void>('send_test_click', { peerId });
}

// --- Workspace commands ---

export interface WorkspaceDto {
  id: string;
  name: string;
  owner_device_id: string;
  is_mirror: boolean;
  is_orphan: boolean;
  owner_username: string | null;
  member_count: number;
  version: number;
}

export interface WorkspaceMemberDto {
  device_id: string;
  public_key_hex: string;
  username: string;
  is_owner: boolean;
}

export interface PendingInviteDto {
  invite_id: string;
  workspace_id: string;
  workspace_name: string;
  sender_device_id: string;
  sender_username: string;
  sender_fingerprint_hex: string;
}

export async function listWorkspaces(): Promise<WorkspaceDto[]> {
  return await invoke<WorkspaceDto[]>('list_workspaces');
}

export async function createWorkspace(name: string, peerIds: string[]): Promise<WorkspaceDto> {
  return await invoke<WorkspaceDto>('create_workspace', { name, peerIds });
}

export async function inviteToWorkspace(workspaceId: string, peerId: string): Promise<string> {
  return await invoke<string>('invite_to_workspace', { workspaceId, peerId });
}

export async function acceptInvite(inviteId: string): Promise<WorkspaceDto> {
  return await invoke<WorkspaceDto>('accept_invite', { inviteId });
}

export async function rejectInvite(inviteId: string): Promise<void> {
  return await invoke<void>('reject_invite', { inviteId });
}

export async function connectToWorkspace(workspaceId: string): Promise<void> {
  return await invoke<void>('connect_to_workspace', { workspaceId });
}

export async function disconnectFromWorkspace(): Promise<void> {
  return await invoke<void>('disconnect_from_workspace');
}

export async function forceDisconnectAndConnect(workspaceId: string): Promise<void> {
  return await invoke<void>('force_disconnect_and_connect', { workspaceId });
}

export async function deleteWorkspace(workspaceId: string): Promise<void> {
  return await invoke<void>('delete_workspace', { workspaceId });
}

export async function renameWorkspace(workspaceId: string, newName: string): Promise<WorkspaceDto> {
  return invoke('rename_workspace', { workspaceId, newName });
}

export async function addWorkspaceMember(input: {
  workspaceId: string;
  deviceId: string;
  publicKeyHex: string;
  username: string;
}): Promise<WorkspaceDto> {
  return invoke('add_workspace_member', { input });
}

export async function removeWorkspaceMember(
  workspaceId: string,
  deviceId: string,
): Promise<WorkspaceDto> {
  return invoke('remove_workspace_member', { workspaceId, deviceId });
}

export async function forgetWorkspace(workspaceId: string): Promise<void> {
  return invoke('forget_workspace', { workspaceId });
}

export async function listWorkspaceMembers(workspaceId: string): Promise<WorkspaceMemberDto[]> {
  return invoke('list_workspace_members', { workspaceId });
}

export interface MonitorRectDto {
  id: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EdgeConfigDto {
  local_exit: string;
  remote_entry: string;
  exit_local_monitor_id?: number;
}

export interface MonitorLayoutDto {
  local_monitors: MonitorRectDto[];
  remote_peer: string;
  remote_virtual: MonitorRectDto;
  remote_monitors?: MonitorRectDto[];
  edge: EdgeConfigDto;
}

export interface SessionDesktopLayoutDto {
  per_device: Record<string, MonitorRectDto[]>;
}

export interface WorkspaceLayoutDto {
  per_device: Record<string, MonitorLayoutDto>;
}

export async function getWorkspaceLayout(workspaceId: string): Promise<WorkspaceLayoutDto> {
  return invoke('get_workspace_layout', { workspaceId });
}

export async function updateWorkspaceLayout(input: {
  workspaceId: string;
  deviceId: string;
  layout: MonitorLayoutDto;
}): Promise<WorkspaceDto> {
  return invoke('update_workspace_layout', {
    input: {
      workspace_id: input.workspaceId,
      device_id: input.deviceId,
      layout: input.layout,
    },
  });
}

export async function listLocalMonitors(): Promise<MonitorRectDto[]> {
  return invoke('list_local_monitors');
}

export async function getKvmLayout(peerId: string): Promise<MonitorLayoutDto | null> {
  return invoke('get_kvm_layout', { peerId });
}

export async function getKvmSessionLayout(
  peerId: string,
): Promise<SessionDesktopLayoutDto | null> {
  return invoke('get_kvm_session_layout', { peerId });
}

export async function getPeerMonitors(
  peerId: string,
  workspaceId?: string,
): Promise<MonitorRectDto[]> {
  return invoke('get_peer_monitors', { peerId, workspaceId: workspaceId ?? null });
}

export async function updateKvmLayout(input: {
  peerId: string;
  layout: MonitorLayoutDto;
}): Promise<void> {
  return invoke('update_kvm_layout', {
    input: {
      peer_id: input.peerId,
      layout: input.layout,
    },
  });
}

export async function updateKvmSessionLayout(input: {
  peerId: string;
  layout: SessionDesktopLayoutDto;
}): Promise<void> {
  return invoke('update_kvm_session_layout', {
    input: {
      peer_id: input.peerId,
      layout: input.layout,
    },
  });
}
