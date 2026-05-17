import { useEffect, useState } from 'react';
import { Code, Stack, Text, Title } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { DeviceCard } from '../components/DeviceCard';
import { PairingModal } from '../components/PairingModal';
import { PeersPanel } from '../components/PeersPanel';
import { getAppInfo, type AppInfo, type DiscoveredPeer } from '../ipc/commands';

export function HomePage() {
  const { t } = useTranslation('common');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [pairingTarget, setPairingTarget] = useState<DiscoveredPeer | null>(null);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch((err: unknown) => {
        console.error('app_info failed', err);
      });
  }, []);

  return (
    <Stack gap="md" maw={720}>
      <Title order={2}>{t('nav.home')}</Title>

      <DeviceCard />

      <PeersPanel onPairRequest={setPairingTarget} />

      <PairingModal
        target={pairingTarget}
        onClose={() => setPairingTarget(null)}
      />

      {info ? (
        <Stack gap="xs" mt="md">
          <Text size="sm" c="dimmed">
            <strong>{t('app.version_label')}:</strong>{' '}
            <Code>{info.version}</Code>
          </Text>
          <Text size="sm" c="dimmed">
            <strong>{t('app.protocol_label')}:</strong>{' '}
            <Code>v{info.protocol_version}</Code>
          </Text>
        </Stack>
      ) : (
        <Text size="sm" c="dimmed">
          {t('app.loading')}
        </Text>
      )}
    </Stack>
  );
}
