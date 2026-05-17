import { useCallback, useEffect, useState } from 'react';
import {
  Badge,
  Button,
  Card,
  Group,
  Skeleton,
  Stack,
  Text,
  Title,
  Tooltip,
} from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  disconnectPeer,
  getFocusState,
  listConnectionStates,
  listDiscoveredPeers,
  openConnection,
  type ConnectionStateDto,
  type DiscoveredPeer,
  type FocusStateDto,
} from '../ipc/commands';
import { onWinxEvent } from '../ipc/events';
import { notifyDomainError } from '../lib/parseDomainError';
import { ConnectionStatus, type ConnectionUiState } from './ConnectionStatus';

function mapDtoStatus(
  status: ConnectionStateDto['status'],
): ConnectionUiState {
  if (status === 'disconnected') return 'disconnected';
  return status;
}

interface PeerConnectionState {
  status: ConnectionUiState;
  rttMs?: number;
  txBytes?: number;
  rxBytes?: number;
}

interface PeerCardProps {
  peer: DiscoveredPeer;
  connection: PeerConnectionState;
  focus: FocusStateDto | null;
  onPair: (peer: DiscoveredPeer) => void;
  onConnect: (peerId: string) => void;
  onDisconnect: (peerId: string) => void;
}

function focusBadgeLabel(
  t: (key: string) => string,
  focus: FocusStateDto | null,
  peerId: string,
): string | null {
  if (!focus) return null;
  if (focus.lock_mode) return t('input.lock_mode_on');
  if (focus.target === 'local') return t('input.focus_local');
  if (focus.target === peerId) return t('input.focus_remote');
  return null;
}

function PeerCard({ peer, connection, focus, onPair, onConnect, onDisconnect }: PeerCardProps) {
  const { t } = useTranslation('common');
  const isConnected = connection.status === 'connected';
  const isBusy =
    connection.status === 'connecting' || connection.status === 'reconnecting';
  const focusLabel = focusBadgeLabel(t, focus, peer.id);

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="xs">
        <Group justify="space-between" align="center">
          <Text fw={600} size="lg">{peer.username}</Text>
          <Group gap="xs">
            <Badge color="blue" variant="light" size="sm">
              {t('discovery.status_online')}
            </Badge>
            {peer.is_paired && (
              <Badge color="teal" variant="light" size="sm">
                {t('discovery.status_paired')}
              </Badge>
            )}
            <ConnectionStatus
              state={connection.status}
              rttMs={connection.rttMs}
              txBytes={connection.txBytes}
              rxBytes={connection.rxBytes}
            />
            {isConnected && focusLabel && (
              <Badge color="grape" variant="light" size="sm">
                {focusLabel}
              </Badge>
            )}
          </Group>
        </Group>

        <Group gap="xs">
          {!peer.is_paired && (
            <Button size="xs" variant="light" onClick={() => onPair(peer)}>
              {t('pairing.pair_button')}
            </Button>
          )}
          {isConnected ? (
            <Button
              size="xs"
              variant="light"
              color="red"
              onClick={() => onDisconnect(peer.id)}
            >
              {t('transport.disconnect_button')}
            </Button>
          ) : (
            <Button
              size="xs"
              variant="filled"
              loading={isBusy}
              disabled={!peer.is_paired}
              title={
                !peer.is_paired
                  ? t('error.transport.peer_not_trusted')
                  : undefined
              }
              onClick={() => onConnect(peer.id)}
            >
              {t('transport.connect_button')}
            </Button>
          )}
        </Group>

        <Tooltip label={t('discovery.peer_fingerprint_hint')} withArrow>
          <Text
            size="xs"
            c="dimmed"
            ff="monospace"
            style={{ cursor: 'default', letterSpacing: '0.05em' }}
          >
            {peer.fingerprint || '—'}
          </Text>
        </Tooltip>

        {peer.addresses.length > 0 && (
          <Tooltip label={peer.addresses.join('\n')} multiline withArrow>
            <Text size="xs" c="dimmed">
              {t('discovery.addresses')}: {peer.addresses[0]}
              {peer.addresses.length > 1 && ` +${peer.addresses.length - 1}`}
            </Text>
          </Tooltip>
        )}
      </Stack>
    </Card>
  );
}

interface PeersPanelProps {
  onPairRequest: (peer: DiscoveredPeer) => void;
}

export function PeersPanel({ onPairRequest }: PeersPanelProps) {
  const { t } = useTranslation('common');
  const [peers, setPeers] = useState<DiscoveredPeer[] | null>(null);
  const [connections, setConnections] = useState<Record<string, PeerConnectionState>>({});
  const [focus, setFocus] = useState<FocusStateDto | null>(null);
  const refreshFocus = useCallback(() => {
    getFocusState()
      .then(setFocus)
      .catch((err: unknown) => {
        console.error('get_focus_state failed', err);
      });
  }, []);

  const syncConnectionsFromBackend = useCallback(async () => {
    try {
      const states = await listConnectionStates();
      setConnections((prev) => {
        const next = { ...prev };
        for (const s of states) {
          next[s.peer_id] = {
            ...prev[s.peer_id],
            status: mapDtoStatus(s.status),
            rttMs: s.rtt_ms,
            txBytes: s.tx_bytes,
            rxBytes: s.rx_bytes,
          };
        }
        return next;
      });
    } catch (err: unknown) {
      console.error('list_connection_states failed', err);
    }
  }, []);

  const refresh = useCallback(() => {
    listDiscoveredPeers()
      .then(setPeers)
      .catch((err: unknown) => {
        console.error('list_discovered_peers failed', err);
        setPeers([]);
      });
    void syncConnectionsFromBackend();
  }, [syncConnectionsFromBackend]);

  const updateConnection = useCallback(
    (peerId: string, patch: Partial<PeerConnectionState>) => {
      setConnections((prev) => ({
        ...prev,
        [peerId]: {
          status: prev[peerId]?.status ?? 'idle',
          ...prev[peerId],
          ...patch,
        },
      }));
    },
    [],
  );

  const handleConnect = useCallback(
    async (peerId: string) => {
      updateConnection(peerId, { status: 'connecting' });
      try {
        await openConnection(peerId);
        await syncConnectionsFromBackend();
      } catch (err) {
        console.error('open_connection failed', err);
        notifyDomainError(err, t);
        updateConnection(peerId, { status: 'error' });
      }
    },
    [t, updateConnection, syncConnectionsFromBackend],
  );

  const handleDisconnect = useCallback(
    async (peerId: string) => {
      try {
        await disconnectPeer(peerId);
        updateConnection(peerId, { status: 'disconnected' });
      } catch (err) {
        console.error('disconnect_peer failed', err);
        notifyDomainError(err, t);
      }
    },
    [t, updateConnection],
  );

  useEffect(() => {
    refresh();

    const unlisten = onWinxEvent((event) => {
      if (event.kind === 'peers-updated' || event.kind === 'pairing-completed') {
        refresh();
      }
      if (event.kind === 'connection-established' && event.peer_id) {
        void syncConnectionsFromBackend();
        refreshFocus();
      }
      if (event.kind === 'focus-switched' || event.kind === 'hotkey-triggered') {
        refreshFocus();
      }
      if (event.kind === 'connection-lost' && event.peer_id) {
        void syncConnectionsFromBackend();
      }
      if (event.kind === 'stats-updated' && event.peer_id) {
        updateConnection(event.peer_id, {
          status: 'connected',
          rttMs: event.rtt_ms,
          txBytes: event.tx_bytes,
          rxBytes: event.rx_bytes,
        });
      }
    });

    return () => {
      unlisten.then((u) => u());
    };
  }, [refresh, refreshFocus, syncConnectionsFromBackend, updateConnection]);

  return (
    <Stack gap="sm">
      <Title order={3}>{t('discovery.section_title')}</Title>

      {peers === null ? (
        <>
          <Skeleton height={80} radius="md" />
          <Text size="sm" c="dimmed">{t('discovery.scanning')}</Text>
        </>
      ) : peers.length === 0 ? (
        <Card withBorder radius="md" p="md">
          <Text size="sm" c="dimmed" ta="center">
            {t('discovery.empty')}
          </Text>
        </Card>
      ) : (
        peers.map((peer) => (
          <PeerCard
            key={peer.id}
            peer={peer}
            connection={connections[peer.id] ?? { status: 'idle' }}
            focus={focus}
            onPair={onPairRequest}
            onConnect={handleConnect}
            onDisconnect={handleDisconnect}
          />
        ))
      )}
    </Stack>
  );
}
