import { useEffect, useMemo, useState } from 'react';
import { Alert, Stack, Title } from '@mantine/core';
import { IconInfoCircle } from '@tabler/icons-react';
import { useTranslation } from 'react-i18next';

import { ConnectivitySuitePanel } from '../components/lab/ConnectivitySuitePanel';
import { KeyboardMirrorPanel } from '../components/lab/KeyboardMirrorPanel';
import { PeerSelector } from '../components/lab/PeerSelector';
import { listConnectionStates, type ConnectionStateDto } from '../ipc/commands';

export function LabPage() {
  const { t } = useTranslation('lab');
  const [peerId, setPeerId] = useState<string | null>(null);
  const [connections, setConnections] = useState<ConnectionStateDto[]>([]);

  useEffect(() => {
    listConnectionStates()
      .then(setConnections)
      .catch((err: unknown) => console.error('list_connection_states', err));
    const interval = setInterval(() => {
      listConnectionStates()
        .then(setConnections)
        .catch((err: unknown) => console.error('list_connection_states', err));
    }, 2000);
    return () => clearInterval(interval);
  }, []);

  const peerConnected = useMemo(
    () =>
      peerId != null &&
      connections.some((c) => c.peer_id === peerId && c.status === 'connected'),
    [peerId, connections],
  );

  return (
    <Stack gap="md" maw={900}>
      <Title order={2}>{t('title')}</Title>

      <Alert icon={<IconInfoCircle size={18} />} variant="light" color="blue">
        {t('hint_single_connect')}
      </Alert>

      <PeerSelector peerId={peerId} onPeerIdChange={setPeerId} />

      <ConnectivitySuitePanel peerId={peerId} peerConnected={peerConnected} />

      <KeyboardMirrorPanel peerId={peerId} peerConnected={peerConnected} />
    </Stack>
  );
}
