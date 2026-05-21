import { useEffect, useMemo, useState } from 'react';
import { Badge, Select, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  listConnectionStates,
  listDiscoveredPeers,
  type ConnectionStateDto,
  type DiscoveredPeer,
} from '../../ipc/commands';

interface PeerSelectorProps {
  peerId: string | null;
  onPeerIdChange: (peerId: string | null) => void;
}

export function PeerSelector({ peerId, onPeerIdChange }: PeerSelectorProps) {
  const { t } = useTranslation('lab');
  const [peers, setPeers] = useState<DiscoveredPeer[]>([]);
  const [connections, setConnections] = useState<ConnectionStateDto[]>([]);

  useEffect(() => {
    listDiscoveredPeers()
      .then(setPeers)
      .catch((err: unknown) => console.error('list_discovered_peers', err));
    listConnectionStates()
      .then(setConnections)
      .catch((err: unknown) => console.error('list_connection_states', err));
  }, []);

  const pairedPeers = useMemo(() => peers.filter((p) => p.is_paired), [peers]);

  const connectedSet = useMemo(
    () =>
      new Set(
        connections.filter((c) => c.status === 'connected').map((c) => c.peer_id),
      ),
    [connections],
  );

  const selectData = pairedPeers.map((p) => ({
    value: p.id,
    label: p.username,
  }));

  const selectedConnected = peerId ? connectedSet.has(peerId) : false;

  if (pairedPeers.length === 0) {
    return (
      <Text size="sm" c="dimmed">
        {t('peer_select.none_paired')}
      </Text>
    );
  }

  return (
    <Stack gap="xs">
      <Select
        label={t('peer_select.label')}
        placeholder={t('peer_select.placeholder')}
        data={selectData}
        value={peerId}
        onChange={onPeerIdChange}
        allowDeselect
      />
      {peerId && (
        <Badge
          color={selectedConnected ? 'teal' : 'gray'}
          variant="light"
          w="fit-content"
        >
          {selectedConnected
            ? t('peer_select.connected')
            : t('peer_select.not_connected')}
        </Badge>
      )}
    </Stack>
  );
}
