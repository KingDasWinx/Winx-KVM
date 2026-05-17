import { useEffect, useState } from 'react';
import { Button, Center, Group, Loader, Notification, PinInput, Stack, Text } from '@mantine/core';
import { IconAlertCircle, IconCheck } from '@tabler/icons-react';
import { useTranslation } from 'react-i18next';

import { submitPin } from '../ipc/commands';
import { onWinxEvent } from '../ipc/events';

type ToastPhase =
  | { kind: 'idle' }
  | {
      kind: 'waiting-pin';
      sessionId: string;
      peerId: string;
      peerUsername: string;
      pinValue: string;
    }
  | { kind: 'submitting'; sessionId: string; peerId: string }
  | { kind: 'success'; peerUsername: string }
  | { kind: 'error'; message: string };

export function IncomingPairingToast() {
  const { t } = useTranslation('common');
  const [phase, setPhase] = useState<ToastPhase>({ kind: 'idle' });

  // Escuta pedido de pareamento recebido via rede (lado responder)
  useEffect(() => {
    const unlistenPromise = onWinxEvent((event) => {
      if (event.kind === 'pairing-incoming') {
        setPhase({
          kind: 'waiting-pin',
          sessionId: event.session_id,
          peerId: event.peer_id,
          peerUsername: event.peer_username,
          pinValue: '',
        });
      }
    });

    return () => {
      unlistenPromise.then((u) => u());
    };
  }, []);

  // Escuta conclusão ou cancelamento
  useEffect(() => {
    const unlistenPromise = onWinxEvent((event) => {
      if (event.kind === 'pairing-completed') {
        setPhase({ kind: 'success', peerUsername: event.peer_username });
        setTimeout(() => setPhase({ kind: 'idle' }), 2000);
      } else if (event.kind === 'pairing-cancelled' || event.kind === 'pairing-failed') {
        setPhase({ kind: 'error', message: t('error.pairing.pin_expired') });
        setTimeout(() => setPhase({ kind: 'idle' }), 3000);
      }
    });

    return () => {
      unlistenPromise.then((u) => u());
    };
  }, [t]);

  const handleSubmitPin = async () => {
    if (phase.kind !== 'waiting-pin' || phase.pinValue.length !== 6) return;

    setPhase({ kind: 'submitting', sessionId: phase.sessionId, peerId: phase.peerId });

    try {
      await submitPin(phase.sessionId, phase.pinValue);
      // pairing-completed chegará via evento e atualizará a UI
    } catch (err: unknown) {
      const msg = typeof err === 'string' ? err : t('error.general.internal_error');
      setPhase({ kind: 'error', message: msg });
      setTimeout(() => setPhase({ kind: 'idle' }), 3000);
    }
  };

  const handleCancel = () => {
    setPhase({ kind: 'idle' });
  };

  if (phase.kind === 'idle') {
    return null;
  }

  return (
    <Notification
      icon={
        phase.kind === 'success' ? (
          <IconCheck size={20} />
        ) : phase.kind === 'error' ? (
          <IconAlertCircle size={20} />
        ) : undefined
      }
      color={phase.kind === 'success' ? 'green' : phase.kind === 'error' ? 'red' : 'blue'}
      title={t('pairing.incoming_title')}
      style={{
        position: 'fixed',
        bottom: 24,
        left: 24,
        zIndex: 300,
        width: 340,
        boxShadow: '0 4px 12px rgba(0, 0, 0, 0.15)',
      }}
      onClose={handleCancel}
    >
      <Stack gap="md">
        {phase.kind === 'waiting-pin' && (
          <>
            <Text size="sm">{t('pairing.incoming_hint', { username: phase.peerUsername })}</Text>
            <Center>
              <PinInput
                length={6}
                type="number"
                placeholder={t('pairing.pin_input_placeholder')}
                value={phase.pinValue}
                onChange={(val) =>
                  setPhase((prev) => {
                    if (prev.kind === 'waiting-pin') {
                      return { ...prev, pinValue: val };
                    }
                    return prev;
                  })
                }
                autoFocus
              />
            </Center>
            <Group justify="flex-end">
              <Button variant="subtle" onClick={handleCancel}>
                {t('pairing.cancel')}
              </Button>
              <Button onClick={handleSubmitPin} disabled={phase.pinValue.length !== 6}>
                {t('pairing.ok')}
              </Button>
            </Group>
          </>
        )}

        {phase.kind === 'submitting' && (
          <Center py="md">
            <Loader size="sm" />
          </Center>
        )}

        {phase.kind === 'success' && (
          <Text ta="center" c="green">
            ✓ {t('pairing.success', { username: phase.peerUsername })}
          </Text>
        )}

        {phase.kind === 'error' && <Text ta="center" c="red">{phase.message}</Text>}
      </Stack>
    </Notification>
  );
}
