import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { Button, Loader, Stack, Text, Title } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { onWinxEvent } from '../../ipc/events';
import WorkspaceCard from './WorkspaceCard';
import CreateWorkspaceModal from './CreateWorkspaceModal';
import WorkspaceDetailDrawer from './WorkspaceDetailDrawer';
import type { WorkspaceDto } from '../../ipc/commands';

export default function WorkspacesPanel() {
  const { t } = useTranslation('workspace');
  const { workspaces, refresh, setPresence, setActiveWorkspaceId } = useWorkspaceStore();
  const [isCreating, setIsCreating] = useState(false);
  const [detailWs, setDetailWs] = useState<WorkspaceDto | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const doRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await refresh();
    } catch (err) {
      console.error('Failed to refresh workspaces:', err);
      notifications.show({
        title: t('toast.refreshError.title'),
        message: t('toast.refreshError.message'),
        color: 'red',
      });
    } finally {
      setRefreshing(false);
    }
  }, [refresh, t]);

  useEffect(() => {
    void doRefresh();

    let unlisten: (() => void) | undefined;

    void onWinxEvent((event) => {
      if (event.kind === 'workspace-member-presence') {
        setPresence(event.workspace_id, event.peer_id, event.is_online);
        return;
      }

      if (event.kind === 'workspace-connected') {
        setActiveWorkspaceId(event.workspace_id);
      }

      if (event.kind === 'workspace-disconnected') {
        setActiveWorkspaceId(null);
      }

      if (
        event.kind === 'workspaces-updated' ||
        event.kind === 'workspace-marked-orphan' ||
        event.kind === 'workspace-connected' ||
        event.kind === 'workspace-disconnected' ||
        event.kind === 'workspace-invite-accepted' ||
        event.kind === 'workspace-invite-rejected' ||
        event.kind === 'workspace-invite-expired'
      ) {
        void doRefresh();
        if (
          event.kind === 'workspaces-updated' &&
          event.sync_from_remote &&
          event.new_version !== undefined
        ) {
          notifications.show({
            title: t('toast.syncApplied.title'),
            message: t('toast.syncApplied.message', {
              name: event.workspace_name ?? t('toast.syncApplied.unknownName'),
            }),
            color: 'blue',
          });
        }
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [doRefresh, setActiveWorkspaceId, setPresence, t]);

  useEffect(() => {
    if (!detailWs) return;
    const updated = workspaces.find((w) => w.id === detailWs.id);
    if (updated && updated.version !== detailWs.version) {
      setDetailWs(updated);
    }
  }, [workspaces, detailWs]);

  return (
    <>
      <Stack gap="md" id="workspaces-panel">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <Title order={3}>{t('title')}</Title>
          <GroupWithLoader refreshing={refreshing}>
            <Button onClick={() => setIsCreating(true)} disabled={refreshing}>
              {t('newButton')}
            </Button>
          </GroupWithLoader>
        </div>

        <Text size="xs" c="dimmed">{t('hotkeyHint')}</Text>

        {refreshing && workspaces.length === 0 ? (
          <Loader size="sm" />
        ) : workspaces.length === 0 ? (
          <Text c="dimmed">{t('empty')}</Text>
        ) : (
          <Stack gap="sm" style={{ opacity: refreshing ? 0.6 : 1 }}>
            {workspaces.map((ws) => (
              <WorkspaceCard
                key={ws.id}
                workspace={ws}
                onOpenDetail={() => setDetailWs(ws)}
              />
            ))}
          </Stack>
        )}
      </Stack>

      <CreateWorkspaceModal opened={isCreating} onClose={() => setIsCreating(false)} />
      <WorkspaceDetailDrawer workspace={detailWs} onClose={() => setDetailWs(null)} />
    </>
  );
}

function GroupWithLoader({
  refreshing,
  children,
}: {
  refreshing: boolean;
  children: ReactNode;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {refreshing && <Loader size="xs" />}
      {children}
    </div>
  );
}
