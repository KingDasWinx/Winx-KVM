import { useCallback, useEffect, useState } from 'react';
import {
  Button,
  Card,
  Stack,
  Table,
  Text,
  Title,
} from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  getInputDebugStats,
  type InputDebugStatsDto,
} from '../../ipc/commands';

interface InputDebugPanelProps {
  mirrorActive: boolean;
}

export function InputDebugPanel({ mirrorActive }: InputDebugPanelProps) {
  const { t } = useTranslation('lab');
  const [stats, setStats] = useState<InputDebugStatsDto | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(() => {
    setLoading(true);
    getInputDebugStats()
      .then(setStats)
      .catch((err: unknown) => console.error('get_input_debug_stats', err))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!mirrorActive) return;
    const id = setInterval(refresh, 500);
    return () => clearInterval(id);
  }, [mirrorActive, refresh]);

  const rows: { key: string; value: string | number | boolean }[] = stats
    ? [
        { key: t('debug.keys_sent'), value: stats.keys_sent },
        { key: t('debug.keys_hooked'), value: stats.keys_hooked },
        { key: t('debug.keys_send_errors'), value: stats.keys_send_errors },
        { key: t('debug.remote_received'), value: stats.remote_frames_received },
        { key: t('debug.remote_inject_ok'), value: stats.remote_inject_ok },
        { key: t('debug.remote_inject_fail'), value: stats.remote_inject_fail },
        { key: t('debug.input_enabled'), value: stats.input_enabled },
        { key: t('debug.has_input_tx'), value: stats.has_input_tx },
      ]
    : [];

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="md">
        <Title order={4}>{t('debug.title')}</Title>
        <Text size="sm" c="dimmed">
          {t('debug.hint_two_pcs')}
        </Text>
        <Text size="xs" c="dimmed">
          {t('debug.hint_logs')}
        </Text>
        <Button variant="light" onClick={refresh} loading={loading}>
          {t('debug.refresh')}
        </Button>
        {stats && (
          <Table striped withTableBorder>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>{t('debug.metric')}</Table.Th>
                <Table.Th>{t('debug.value')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {rows.map((row) => (
                <Table.Tr key={row.key}>
                  <Table.Td>{row.key}</Table.Td>
                  <Table.Td>
                    <Text size="sm">{String(row.value)}</Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Stack>
    </Card>
  );
}
