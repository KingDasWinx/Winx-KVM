import { Badge, Button, Card, Group, Stack, Text, Tooltip } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import * as ipc from '../../ipc/commands';
import type { WorkspaceDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto;
  onOpenDetail?: () => void;
}

export default function WorkspaceCard({ workspace, onOpenDetail }: Props) {
  const { t } = useTranslation('workspace');
  const { activeWorkspaceId, presence, setConflict } = useWorkspaceStore();
  const isActive = activeWorkspaceId === workspace.id;

  const presencePrefix = `${workspace.id}:`;
  const memberPresence = Object.entries(presence).filter(([key]) =>
    key.startsWith(presencePrefix),
  );
  const isAvailable =
    memberPresence.some(([, online]) => online) ||
    (!workspace.is_mirror && memberPresence.length === 0);

  const borderColor = workspace.is_mirror ? 'var(--mantine-color-gray-4)' : undefined;

  const handleConnect = async () => {
    try {
      await ipc.connectToWorkspace(workspace.id);
    } catch (err: unknown) {
      const errorMsg = typeof err === 'string' ? err : (err as Error)?.message;
      if (errorMsg && errorMsg.includes('workspace.conflict')) {
        try {
          const parsedError = JSON.parse(errorMsg) as { code: string; active_id: string };
          if (parsedError.code === 'workspace.conflict') {
            setConflict({
              activeId: parsedError.active_id,
              targetId: workspace.id,
              activeName: workspace.name,
              targetName: workspace.name,
            });
          }
        } catch {
          console.error('Failed to connect:', err);
        }
      }
    }
  };

  const handleDisconnect = async () => {
    await ipc.disconnectFromWorkspace().catch(console.error);
  };

  const handleDelete = async () => {
    await ipc.deleteWorkspace(workspace.id).catch(console.error);
  };

  const handleForget = async () => {
    await ipc.forgetWorkspace(workspace.id).catch(console.error);
  };

  return (
    <Card withBorder radius="md" p="md" style={{ borderColor }}>
      <Stack gap="xs">
        <Group justify="space-between">
          <Group gap="xs">
            <Text fw={600} onClick={onOpenDetail} style={{ cursor: onOpenDetail ? 'pointer' : undefined }}>
              {workspace.name}
            </Text>
            {workspace.is_mirror && workspace.owner_username && (
              <Badge color="gray" variant="light">
                {t('card.mirrorBadge', { username: workspace.owner_username })}
              </Badge>
            )}
            {workspace.is_orphan && (
              <Tooltip label={t('card.orphanTooltip')}>
                <Badge color="orange" variant="filled">{t('card.orphanBadge')}</Badge>
              </Tooltip>
            )}
            <Badge color={isAvailable ? 'green' : 'gray'} variant="dot">
              {isAvailable ? t('card.available') : t('card.unavailable')}
            </Badge>
          </Group>
        </Group>
        <Text size="sm" c="dimmed">
          {t('card.memberCount_other', { count: workspace.member_count })}
        </Text>
        <Group gap="xs">
          {isActive ? (
            <Button size="xs" variant="light" color="red" onClick={handleDisconnect}>
              {t('card.disconnectButton')}
            </Button>
          ) : (
            <Button size="xs" variant="filled" onClick={handleConnect} disabled={!isAvailable}>
              {t('card.connectButton')}
            </Button>
          )}
          {!workspace.is_mirror && (
            <Button size="xs" variant="subtle" color="red" onClick={handleDelete}>
              {t('card.deleteButton')}
            </Button>
          )}
          {workspace.is_mirror && workspace.is_orphan && (
            <Button size="xs" variant="subtle" color="orange" onClick={handleForget}>
              {t('card.forgetButton')}
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  );
}
