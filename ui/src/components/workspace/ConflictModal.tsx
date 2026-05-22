import { useState } from 'react';
import { Button, Divider, Group, Modal, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import * as ipc from '../../ipc/commands';
import CreateWorkspaceModal from './CreateWorkspaceModal';

export default function ConflictModal() {
  const { t } = useTranslation('workspace');
  const { conflict, setConflict, refresh } = useWorkspaceStore();
  const [loading, setLoading] = useState(false);
  const [creatingNew, setCreatingNew] = useState(false);

  if (!conflict) {
    return null;
  }

  const handleDisconnectAndSwitch = async () => {
    setLoading(true);
    try {
      await ipc.forceDisconnectAndConnect(conflict.targetId);
      await refresh();
      setConflict(null);
    } catch (err) {
      console.error('Failed to switch workspaces:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleCancel = () => {
    setConflict(null);
  };

  const handleCreateNew = () => {
    setConflict(null);
    setCreatingNew(true);
  };

  return (
    <>
      <Modal
        opened={!!conflict}
        onClose={handleCancel}
        title={t('conflictModal.title')}
        centered
      >
        <Stack gap="md">
          <Text>
            {t('conflictModal.body', {
              activeName: conflict.activeName,
              targetName: conflict.targetName,
            })}
          </Text>

          <Group justify="flex-end" gap="xs">
            <Button variant="subtle" onClick={handleCancel} disabled={loading}>
              {t('conflictModal.cancelButton')}
            </Button>
            <Button color="orange" onClick={handleDisconnectAndSwitch} loading={loading}>
              {t('conflictModal.disconnectAndConnectButton')}
            </Button>
          </Group>

          <Divider label={t('conflictModal.createNewSeparator')} labelPosition="center" />

          <Button
            size="lg"
            variant="gradient"
            gradient={{ from: 'blue', to: 'cyan' }}
            fullWidth
            onClick={handleCreateNew}
            disabled={loading}
          >
            {t('conflictModal.createNewButton')}
          </Button>
          <Text size="xs" c="dimmed" ta="center">
            {t('conflictModal.createNewHint')}
          </Text>
        </Stack>
      </Modal>

      <CreateWorkspaceModal
        opened={creatingNew}
        onClose={() => setCreatingNew(false)}
        initialPeerIds={[conflict.activeId, conflict.targetId]}
      />
    </>
  );
}
