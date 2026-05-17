import { useEffect, useRef, useState } from 'react';
import {
  Button,
  Center,
  Group,
  Loader,
  Modal,
  PinInput,
  Progress,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { cancelPairing, initiatePairing, submitPin } from '../ipc/commands';
import { onWinxEvent } from '../ipc/events';

type ModalPhase =
  | { kind: 'idle' }
  | { kind: 'initiating' }
  | { kind: 'showing-pin'; sessionId: string; pin: string; secondsLeft: number }
  | { kind: 'entering-pin'; sessionId: string; peerId: string }
  | { kind: 'submitting' }
  | { kind: 'success'; peerUsername: string }
  | { kind: 'error'; message: string };

interface PairingModalProps {
  /** Peer que o usuário quer parear. Null = modal fechado. */
  target: { id: string; username: string; fingerprint: string } | null;
  onClose: () => void;
}

const TIMEOUT_SECS = 90;

export function PairingModal({ target, onClose }: PairingModalProps) {
  const { t } = useTranslation('common');
  const [phase, setPhase] = useState<ModalPhase>({ kind: 'idle' });
  const [pinValue, setPinValue] = useState('');
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  // Limpa ao fechar
  const handleClose = () => {
    clearTimer();
    if (phase.kind === 'showing-pin') {
      cancelPairing(phase.sessionId).catch(console.error);
    }
    setPhase({ kind: 'idle' });
    setPinValue('');
    onClose();
  };

  // Inicia o countdown de 90s quando entramos em showing-pin
  useEffect(() => {
    if (phase.kind !== 'showing-pin') return;
    clearTimer();

    timerRef.current = setInterval(() => {
      setPhase((prev) => {
        if (prev.kind !== 'showing-pin') return prev;
        const next = prev.secondsLeft - 1;
        if (next <= 0) {
          clearTimer();
          return { kind: 'error', message: t('error.pairing.pin_expired') };
        }
        return { ...prev, secondsLeft: next };
      });
    }, 1000);

    return clearTimer;
  }, [phase.kind, t]);

  // Escuta eventos de pairing
  useEffect(() => {
    const unlistenPromise = onWinxEvent((event) => {
      if (event.kind === 'pairing-completed') {
        clearTimer();
        setPhase({ kind: 'success', peerUsername: event.peer_username });
      }
      if (event.kind === 'pairing-cancelled' || event.kind === 'pairing-failed') {
        clearTimer();
        setPhase({ kind: 'error', message: t('error.pairing.pin_expired') });
      }
      // pairing-initiated do lado responder é tratado pelo pai que abre o modal
    });
    return () => {
      unlistenPromise.then((u) => u());
    };
  }, [t]);

  // Abre automaticamente como initiator quando target muda
  useEffect(() => {
    if (!target) return;
    setPhase({ kind: 'initiating' });
    setPinValue('');

    initiatePairing(target.id, target.username)
      .then(({ session_id, pin }) => {
        setPhase({ kind: 'showing-pin', sessionId: session_id, pin, secondsLeft: TIMEOUT_SECS });
      })
      .catch((err: unknown) => {
        const msg = typeof err === 'string' ? err : t('error.general.internal_error');
        setPhase({ kind: 'error', message: msg });
      });
  }, [target, t]);

  const handleSubmitPin = async (sessionId: string) => {
    if (pinValue.length !== 6 || !target) return;
    setPhase({ kind: 'submitting' });
    try {
      // No MVP o peer_public_key_hex e ephemeral são passados via um placeholder
      // (o handshake real de rede vem no Sprint 5). Por ora usamos zeros.
      await submitPin(sessionId, pinValue, '0'.repeat(64), target.username);
      // PairingCompleted chegará via evento
    } catch (err) {
      const msg = typeof err === 'string' ? err : t('error.general.internal_error');
      setPhase({ kind: 'error', message: msg });
    }
  };

  const progressValue =
    phase.kind === 'showing-pin'
      ? (phase.secondsLeft / TIMEOUT_SECS) * 100
      : 100;

  return (
    <Modal
      opened={!!target}
      onClose={handleClose}
      title={
        target
          ? t('pairing.modal_title_initiator', { username: target.username })
          : ''
      }
      centered
      size="sm"
    >
      <Stack gap="md">
        {phase.kind === 'initiating' && (
          <Center py="xl">
            <Loader />
          </Center>
        )}

        {phase.kind === 'showing-pin' && (
          <>
            <Text size="sm" c="dimmed">{t('pairing.pin_hint')}</Text>
            <Center>
              <Title order={1} ff="monospace" style={{ letterSpacing: '0.2em' }}>
                {phase.pin}
              </Title>
            </Center>
            <Progress value={progressValue} color={phase.secondsLeft > 30 ? 'blue' : 'red'} animated />
            <Text size="xs" c="dimmed" ta="center">
              {t('pairing.countdown', { seconds: phase.secondsLeft })}
            </Text>
            <Group justify="flex-end">
              <Button variant="subtle" color="red" onClick={handleClose}>
                {t('pairing.cancel')}
              </Button>
            </Group>
          </>
        )}

        {phase.kind === 'entering-pin' && (
          <>
            <Text size="sm">{t('pairing.pin_input_label')}</Text>
            <Center>
              <PinInput
                length={6}
                type="number"
                placeholder={t('pairing.pin_input_placeholder')}
                value={pinValue}
                onChange={setPinValue}
                autoFocus
              />
            </Center>
            <Group justify="flex-end">
              <Button variant="subtle" onClick={handleClose}>
                {t('pairing.cancel')}
              </Button>
              <Button
                onClick={() => handleSubmitPin(phase.sessionId)}
                disabled={pinValue.length !== 6}
              >
                {t('pairing.submit')}
              </Button>
            </Group>
          </>
        )}

        {phase.kind === 'submitting' && (
          <Center py="xl">
            <Loader />
          </Center>
        )}

        {phase.kind === 'success' && (
          <>
            <Text ta="center" c="green">
              {t('pairing.success', { username: phase.peerUsername })}
            </Text>
            <Group justify="center">
              <Button onClick={handleClose}>{t('pairing.cancel')}</Button>
            </Group>
          </>
        )}

        {phase.kind === 'error' && (
          <>
            <Text ta="center" c="red">{phase.message}</Text>
            <Group justify="center">
              <Button onClick={handleClose}>{t('pairing.cancel')}</Button>
            </Group>
          </>
        )}
      </Stack>
    </Modal>
  );
}
