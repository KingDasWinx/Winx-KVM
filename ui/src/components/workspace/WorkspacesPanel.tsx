import { useEffect, useState } from 'react';
import { Button, Stack, Text, Title } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { onWinxEvent } from '../../ipc/events';
import type { UnlistenFn } from '@tauri-apps/api/event';
import WorkspaceCard from './WorkspaceCard';
import CreateWorkspaceModal from './CreateWorkspaceModal';

export default function WorkspacesPanel() {
  const { t } = useTranslation('workspace');
  const { workspaces, refresh } = useWorkspaceStore();
  const [isCreating, setIsCreating] = useState(false);
  const [unlistenFn, setUnlistenFn] = useState<UnlistenFn | null>(null);

  useEffect(() => {
    refresh();

    const setupEventListener = async () => {
      const unlisten = await onWinxEvent((event) => {
        if (
          event.kind === 'workspaces-updated' ||
          event.kind === 'workspace-connected' ||
          event.kind === 'workspace-disconnected' ||
          event.kind === 'workspace-invite-accepted' ||
          event.kind === 'workspace-invite-rejected' ||
          event.kind === 'workspace-invite-expired' ||
          event.kind === 'workspace-invite-incoming'
        ) {
          refresh();
        }
      });
      setUnlistenFn(() => unlisten);
    };

    setupEventListener();

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [refresh, unlistenFn]);

  return (
    <>
      <Stack gap="md">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <Title order={3}>{t('title')}</Title>
          <Button onClick={() => setIsCreating(true)}>{t('newButton')}</Button>
        </div>

        {workspaces.length === 0 ? (
          <Text c="dimmed">{t('empty')}</Text>
        ) : (
          <Stack gap="sm">
            {workspaces.map((ws) => (
              <WorkspaceCard key={ws.id} workspace={ws} />
            ))}
          </Stack>
        )}
      </Stack>

      <CreateWorkspaceModal
        opened={isCreating}
        onClose={() => setIsCreating(false)}
      />
    </>
  );
}
