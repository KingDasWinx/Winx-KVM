import { useState } from 'react';
import { Button, Divider, Drawer, Group, Stack, TextInput } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import WorkspaceMembersPanel from './WorkspaceMembersPanel';
import InvitePeerModal from './InvitePeerModal';
import WorkspaceLayoutEditor from './WorkspaceLayoutEditor';
import { useWorkspaceStore } from '../../store/workspaceStore';
import type { WorkspaceDto } from '../../ipc/commands';

interface Props {
  workspace: WorkspaceDto | null;
  onClose: () => void;
}

export default function WorkspaceDetailDrawer({ workspace, onClose }: Props) {
  const { t } = useTranslation('workspace');
  const { refresh, clearWorkspacePresence } = useWorkspaceStore();
  const [newName, setNewName] = useState('');
  const [inviteOpen, setInviteOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);

  if (!workspace) return null;

  const handleRename = async () => {
    if (!newName.trim()) return;
    setRenaming(true);
    try {
      await ipc.renameWorkspace(workspace.id, newName.trim());
      setNewName('');
    } catch (err) {
      console.error('Failed to rename workspace:', err);
      notifications.show({
        title: t('detail.renameErrorTitle'),
        message: t('detail.renameErrorMessage'),
        color: 'red',
      });
    } finally {
      setRenaming(false);
    }
  };

  const handleDelete = async () => {
    await ipc.deleteWorkspace(workspace.id).catch(console.error);
    onClose();
  };

  const handleForget = async () => {
    await ipc.forgetWorkspace(workspace.id).catch(console.error);
    clearWorkspacePresence(workspace.id);
    await refresh().catch(console.error);
    onClose();
  };

  return (
    <>
      <Drawer opened={!!workspace} onClose={onClose} title={workspace.name} position="right" size="md">
        <Stack gap="md">
          {!workspace.is_mirror && (
            <Group>
              <TextInput
                placeholder={t('detail.renamePlaceholder')}
                value={newName}
                onChange={(e) => setNewName(e.currentTarget.value)}
                style={{ flex: 1 }}
                disabled={renaming}
              />
              <Button onClick={handleRename} loading={renaming}>
                {t('detail.renameButton')}
              </Button>
            </Group>
          )}

          <Divider />
          <WorkspaceLayoutEditor workspaceId={workspace.id} />

          {!workspace.is_mirror && <Divider />}

          <WorkspaceMembersPanel workspace={workspace} onInviteClick={() => setInviteOpen(true)} />

          <Divider />

          <Group>
            {workspace.is_mirror ? (
              workspace.is_orphan ? (
                <Button color="orange" variant="light" onClick={handleForget}>
                  {t('detail.forgetButton')}
                </Button>
              ) : (
                <Button color="gray" variant="light" onClick={handleForget}>
                  {t('detail.leaveButton')}
                </Button>
              )
            ) : (
              <Button color="red" variant="light" onClick={handleDelete}>
                {t('detail.deleteButton')}
              </Button>
            )}
          </Group>
        </Stack>
      </Drawer>

      <InvitePeerModal
        workspaceId={inviteOpen ? workspace.id : null}
        onClose={() => setInviteOpen(false)}
      />
    </>
  );
}
