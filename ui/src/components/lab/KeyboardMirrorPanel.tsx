import { useEffect, useRef, useState } from 'react';
import { Button, Card, Group, Stack, Text, Title } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  getKeyboardMirrorStatus,
  sendTestClick,
  startKeyboardMirrorTest,
  type KeyboardMirrorStatusDto,
} from '../../ipc/commands';

interface KeyboardMirrorPanelProps {
  peerId: string | null;
  peerConnected: boolean;
}

export function KeyboardMirrorPanel({
  peerId,
  peerConnected,
}: KeyboardMirrorPanelProps) {
  const { t } = useTranslation('lab');
  const [status, setStatus] = useState<KeyboardMirrorStatusDto | null>(null);
  const [starting, setStarting] = useState(false);
  const [clickSending, setClickSending] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, []);

  function startPolling() {
    if (pollRef.current) clearInterval(pollRef.current);
    pollRef.current = setInterval(() => {
      getKeyboardMirrorStatus()
        .then((s) => {
          setStatus(s);
          if (!s.active && pollRef.current) {
            clearInterval(pollRef.current);
            pollRef.current = null;
          }
        })
        .catch((err: unknown) => console.error('get_keyboard_mirror_status', err));
    }, 200);
  }

  async function handleStart() {
    if (!peerId) return;
    setStarting(true);
    try {
      await startKeyboardMirrorTest(peerId, 5);
      const initial = await getKeyboardMirrorStatus();
      setStatus(initial);
      startPolling();
    } catch (err: unknown) {
      console.error('start_keyboard_mirror_test', err);
    } finally {
      setStarting(false);
    }
  }

  async function handleTestClick() {
    if (!peerId) return;
    setClickSending(true);
    try {
      await sendTestClick(peerId);
    } catch (err: unknown) {
      console.error('send_test_click', err);
    } finally {
      setClickSending(false);
    }
  }

  const mirrorActive = status?.active ?? false;

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="md">
        <Title order={4}>{t('keyboard.title')}</Title>
        <Text size="sm" c="dimmed">
          {t('keyboard.hint')}
        </Text>

        {!peerConnected && peerId && (
          <Text size="sm" c="orange">
            {t('errors.not_connected')}
          </Text>
        )}

        <Group>
          <Button
            onClick={handleStart}
            loading={starting}
            disabled={!peerId || !peerConnected || mirrorActive}
          >
            {mirrorActive
              ? t('keyboard.running', { seconds: status?.seconds_left ?? 0 })
              : t('keyboard.start')}
          </Button>
          <Button
            variant="light"
            onClick={handleTestClick}
            loading={clickSending}
            disabled={!peerId || !peerConnected}
          >
            {t('keyboard.test_click')}
          </Button>
        </Group>

        {status && (mirrorActive || status.keys_sent > 0) && (
          <Text size="sm" c="dimmed">
            {t('keyboard.keys_sent', { count: status.keys_sent })}
          </Text>
        )}
      </Stack>
    </Card>
  );
}
