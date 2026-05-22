import { Badge, Button, Card, Group, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import * as ipc from '../../ipc/commands';
import type { WorkspaceDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto;
}

export default function WorkspaceCard({ workspace }: Props) {
  const { t } = useTranslation('workspace');
  const { activeWorkspaceId, setConflict } = useWorkspaceStore();
  const isActive = activeWorkspaceId === workspace.id;

  const handleConnect = async () => {
    try {
      await ipc.connectToWorkspace(workspace.id);
    } catch (err: any) {
      const errorMsg = typeof err === 'string' ? err : err?.message;
      if (errorMsg && errorMsg.includes('workspace.conflict')) {
        try {
          const parsedError = JSON.parse(errorMsg);
          if (parsedError.code === 'workspace.conflict') {
            const activeWs = workspace; // TODO: get active workspace name
            setConflict({
              activeId: parsedError.active_id,
              targetId: workspace.id,
              activeName: activeWs.name,
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
    try {
      await ipc.disconnectFromWorkspace();
    } catch (err) {
      console.error('Failed to disconnect:', err);
    }
  };

  const handleDelete = async () => {
    try {
      await ipc.deleteWorkspace(workspace.id);
    } catch (err) {
      console.error('Failed to delete workspace:', err);
    }
  };

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="xs">
        <Group justify="space-between">
          <Text fw={600}>{workspace.name}</Text>
          <Badge color={workspace.is_mirror ? 'gray' : 'blue'} variant="light">
            {workspace.is_mirror ? t('card.mirrorBadge', { username: 'Owner' }) : t('card.ownedBadge')}
          </Badge>
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
            <Button size="xs" variant="filled" onClick={handleConnect}>
              {t('card.connectButton')}
            </Button>
          )}
          {!workspace.is_mirror && (
            <Button size="xs" variant="subtle" color="red" onClick={handleDelete}>
              {t('card.deleteButton')}
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  );
}
