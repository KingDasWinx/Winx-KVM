import { useEffect, useState } from 'react';
import { Button, Group, Modal, Progress, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { onWinxEvent } from '../../ipc/events';
import * as ipc from '../../ipc/commands';
import type { UnlistenFn } from '@tauri-apps/api/event';

const INVITE_TIMEOUT_SECS = 90;

export default function IncomingInviteModal() {
  const { t } = useTranslation('workspace');
  const { pendingInvite, setPendingInvite, refresh } = useWorkspaceStore();
  const [secondsLeft, setSecondsLeft] = useState(INVITE_TIMEOUT_SECS);
  const [loading, setLoading] = useState(false);
  const [unlistenFn, setUnlistenFn] = useState<UnlistenFn | null>(null);

  useEffect(() => {
    if (!pendingInvite) {
      setSecondsLeft(INVITE_TIMEOUT_SECS);
      return;
    }

    const setupEventListener = async () => {
      const unlisten = await onWinxEvent((event) => {
        if (event.kind === 'workspace-invite-expired') {
          if (event.invite_id === pendingInvite.invite_id) {
            setPendingInvite(null);
          }
        }
      });
      setUnlistenFn(() => unlisten);
    };

    setupEventListener();

    const timer = setInterval(() => {
      setSecondsLeft((prev) => {
        if (prev <= 1) {
          setPendingInvite(null);
          return INVITE_TIMEOUT_SECS;
        }
        return prev - 1;
      });
    }, 1000);

    return () => {
      clearInterval(timer);
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [pendingInvite, setPendingInvite, unlistenFn]);

  if (!pendingInvite) {
    return null;
  }

  const handleAccept = async () => {
    setLoading(true);
    try {
      await ipc.acceptInvite(pendingInvite.invite_id);
      await refresh();
      setPendingInvite(null);
    } catch (err) {
      console.error('Failed to accept invite:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleReject = async () => {
    setLoading(true);
    try {
      await ipc.rejectInvite(pendingInvite.invite_id);
      setPendingInvite(null);
    } catch (err) {
      console.error('Failed to reject invite:', err);
    } finally {
      setLoading(false);
    }
  };

  const progress = (secondsLeft / INVITE_TIMEOUT_SECS) * 100;

  return (
    <Modal
      opened={!!pendingInvite}
      onClose={() => setPendingInvite(null)}
      title={t('incomingInvite.title')}
      centered
    >
      <Stack gap="md">
        <Text>
          {t('incomingInvite.subtitle', {
            username: pendingInvite.sender_username,
            workspaceName: pendingInvite.workspace_name,
          })}
        </Text>

        <div>
          <Text size="sm" fw={600} mb="xs">
            {t('incomingInvite.fingerprintLabel')}
          </Text>
          <Text size="sm" ff="monospace">
            {pendingInvite.sender_fingerprint_hex}
          </Text>
          <Text size="xs" c="dimmed" mt="xs">
            {t('incomingInvite.fingerprintHint')}
          </Text>
        </div>

        <div>
          <Text size="xs" c="dimmed" mb="xs">
            {t('incomingInvite.expiresIn', { seconds: secondsLeft })}
          </Text>
          <Progress value={progress} color="blue" size="sm" />
        </div>

        <Group justify="flex-end" gap="xs">
          <Button variant="subtle" color="red" onClick={handleReject} disabled={loading}>
            {t('incomingInvite.rejectButton')}
          </Button>
          <Button variant="filled" color="green" onClick={handleAccept} loading={loading}>
            {t('incomingInvite.acceptButton')}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
