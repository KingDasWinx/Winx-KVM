import { useEffect, useState } from 'react';
import { Badge, Code, Group, Stack, Text, Title } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { DeviceCard } from '../components/DeviceCard';
import { PairingModal } from '../components/PairingModal';
import { PeersPanel } from '../components/PeersPanel';
import WorkspacesPanel from '../components/workspace/WorkspacesPanel';
import IncomingInviteModal from '../components/workspace/IncomingInviteModal';
import ConflictModal from '../components/workspace/ConflictModal';
import { useGlobalCursor } from '../hooks/useGlobalCursor';
import { useWorkspaceStore } from '../store/workspaceStore';
import { onWinxEvent } from '../ipc/events';
import { getAppInfo, type AppInfo, type DiscoveredPeer } from '../ipc/commands';

export function HomePage() {
  const { t } = useTranslation('common');
  const { t: tWorkspace } = useTranslation('workspace');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [pairingTarget, setPairingTarget] = useState<DiscoveredPeer | null>(null);
  const { activeWorkspaceId } = useWorkspaceStore();
  const cursor = useGlobalCursor(activeWorkspaceId);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch((err: unknown) => {
        console.error('app_info failed', err);
      });
  }, []);

  useEffect(() => {
    const setupHotkeyListener = async () => {
      const unlisten = await onWinxEvent((event) => {
        if (
          event.kind === 'hotkey-triggered' &&
          event.hotkey_action === 'open_active_workspace'
        ) {
          document.getElementById('workspaces-panel')?.scrollIntoView({ behavior: 'smooth' });
        }
      });
      return unlisten;
    };

    let unlisten: (() => void) | undefined;
    void setupHotkeyListener().then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <Stack gap="md" maw={720}>
      <Group gap="sm" align="center">
        <Title order={2}>{t('nav.home')}</Title>
        {import.meta.env.DEV && activeWorkspaceId && cursor && (
          <Badge variant="outline" color="grape" size="sm">
            {tWorkspace('layoutEditor.cursorDebug', {
              x: cursor.x,
              y: cursor.y,
              seq: cursor.seq,
            })}
          </Badge>
        )}
      </Group>

      <DeviceCard />

      <WorkspacesPanel />

      <PeersPanel onPairRequest={setPairingTarget} />

      <PairingModal
        target={pairingTarget}
        onClose={() => setPairingTarget(null)}
      />

      <IncomingInviteModal />

      <ConflictModal />

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
