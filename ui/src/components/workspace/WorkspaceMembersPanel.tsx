import { useEffect, useState } from 'react';
import { Badge, Button, Group, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import { useWorkspaceStore } from '../../store/workspaceStore';
import type { WorkspaceDto, WorkspaceMemberDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto;
  onInviteClick: () => void;
}

export default function WorkspaceMembersPanel({ workspace, onInviteClick }: Props) {
  const { t } = useTranslation('workspace');
  const presence = useWorkspaceStore((s) => s.presence);
  const [members, setMembers] = useState<WorkspaceMemberDto[]>([]);

  useEffect(() => {
    ipc.listWorkspaceMembers(workspace.id).then(setMembers).catch(console.error);
  }, [workspace.id, workspace.version]);

  const handleRemove = async (deviceId: string) => {
    await ipc.removeWorkspaceMember(workspace.id, deviceId).catch(console.error);
    const refreshed = await ipc.listWorkspaceMembers(workspace.id);
    setMembers(refreshed);
  };

  return (
    <Stack gap="xs">
      <Group justify="space-between">
        <Text fw={600}>{t('members.title')}</Text>
        {!workspace.is_mirror && (
          <Button size="xs" onClick={onInviteClick}>
            {t('members.inviteButton')}
          </Button>
        )}
      </Group>
      {members.map((m) => {
        const isOnline =
          presence[`${workspace.id}:${m.device_id}`] === true ||
          (m.is_owner && !workspace.is_mirror);
        return (
          <Group key={m.device_id} justify="space-between">
            <Group gap="xs">
              <Text>{m.username}</Text>
              {m.is_owner && (
                <Badge size="xs" color="blue">
                  {t('members.ownerLabel')}
                </Badge>
              )}
              <Badge size="xs" color={isOnline ? 'green' : 'gray'} variant="dot">
                {isOnline ? t('members.online') : t('members.offline')}
              </Badge>
            </Group>
            {!workspace.is_mirror && !m.is_owner && (
              <Button
                size="xs"
                variant="subtle"
                color="red"
                onClick={() => handleRemove(m.device_id)}
              >
                {t('members.removeButton')}
              </Button>
            )}
          </Group>
        );
      })}
    </Stack>
  );
}
