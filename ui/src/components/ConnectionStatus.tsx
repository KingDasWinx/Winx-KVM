import { Badge, Group, Text, Tooltip } from '@mantine/core';
import { useTranslation } from 'react-i18next';

export type ConnectionUiState =
  | 'idle'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'disconnected'
  | 'error';

interface ConnectionStatusProps {
  state: ConnectionUiState;
  rttMs?: number | null;
  txBytes?: number | null;
  rxBytes?: number | null;
}

export function ConnectionStatus({ state, rttMs, txBytes, rxBytes }: ConnectionStatusProps) {
  const { t } = useTranslation('common');

  const color =
    state === 'connected'
      ? 'green'
      : state === 'connecting' || state === 'reconnecting'
        ? 'yellow'
        : state === 'disconnected' || state === 'error'
          ? 'red'
          : 'gray';

  const label =
    state === 'connected'
      ? t('transport.status_connected')
      : state === 'connecting'
        ? t('transport.status_connecting')
        : state === 'reconnecting'
          ? t('transport.status_reconnecting')
          : t('transport.status_disconnected');

  return (
    <Group gap="xs">
      <Badge color={color} variant="light" size="sm">
        {label}
      </Badge>
      {state === 'connected' && rttMs != null && (
        <Tooltip
          label={t('transport.stats_label', {
            tx: txBytes ?? 0,
            rx: rxBytes ?? 0,
          })}
          withArrow
        >
          <Text size="xs" c="dimmed">
            {t('transport.rtt_label', { ms: rttMs })}
          </Text>
        </Tooltip>
      )}
    </Group>
  );
}
