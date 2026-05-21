import { useState } from 'react';
import {
  Button,
  Card,
  Group,
  Stack,
  Table,
  Text,
  Title,
} from '@mantine/core';
import { IconCheck, IconX } from '@tabler/icons-react';
import { useTranslation } from 'react-i18next';

import {
  runConnectivitySuite,
  type LabProbeResultsDto,
  type ProbeResultDto,
} from '../../ipc/commands';

interface ConnectivitySuitePanelProps {
  peerId: string | null;
  peerConnected: boolean;
}

const SERVICE_I18N: Record<string, string> = {
  mdns: 'connectivity.service.mdns',
  pairing_udp: 'connectivity.service.pairing_udp',
  quic: 'connectivity.service.quic',
  quic_control: 'connectivity.service.quic_control',
  input_stream: 'connectivity.service.input_stream',
};

function serviceLabel(t: (key: string) => string, service: string): string {
  const key = SERVICE_I18N[service];
  return key ? t(key) : service;
}

export function ConnectivitySuitePanel({
  peerId,
  peerConnected,
}: ConnectivitySuitePanelProps) {
  const { t } = useTranslation('lab');
  const [results, setResults] = useState<LabProbeResultsDto | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleRunAll() {
    if (!peerId) {
      setError(t('errors.select_peer'));
      return;
    }
    setError(null);
    setRunning(true);
    try {
      const out = await runConnectivitySuite(peerId);
      setResults(out);
    } catch (err: unknown) {
      console.error('run_connectivity_suite', err);
      setError(t('errors.suite_failed'));
    } finally {
      setRunning(false);
    }
  }

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="md">
        <Group justify="space-between" align="center">
          <Title order={4}>{t('connectivity.title')}</Title>
          <Button
            onClick={handleRunAll}
            loading={running}
            disabled={!peerId}
          >
            {running ? t('connectivity.running') : t('connectivity.run_all')}
          </Button>
        </Group>

        {!peerConnected && peerId && (
          <Text size="sm" c="orange">
            {t('errors.not_connected')}
          </Text>
        )}

        {error && (
          <Text size="sm" c="red">
            {error}
          </Text>
        )}

        {results && results.probes.length > 0 && (
          <ProbeTable probes={results.probes} />
        )}
      </Stack>
    </Card>
  );
}

function ProbeTable({ probes }: { probes: ProbeResultDto[] }) {
  const { t } = useTranslation('lab');

  const rows = probes.map((probe) => (
    <Table.Tr key={probe.service}>
      <Table.Td>
        <Group gap="xs" wrap="nowrap">
          {probe.ok ? (
            <IconCheck size={18} color="var(--mantine-color-teal-6)" />
          ) : (
            <IconX size={18} color="var(--mantine-color-red-6)" />
          )}
          <Text size="sm">{serviceLabel(t, probe.service)}</Text>
        </Group>
      </Table.Td>
      <Table.Td>
        <Text size="sm" c={probe.ok ? 'teal' : 'red'}>
          {probe.ok ? t('connectivity.status.ok') : t('connectivity.status.fail')}
        </Text>
      </Table.Td>
      <Table.Td>
        {probe.latency_ms != null ? (
          <Text size="sm" c="dimmed">
            {t('connectivity.latency', { ms: probe.latency_ms })}
          </Text>
        ) : (
          <Text size="sm" c="dimmed">
            —
          </Text>
        )}
      </Table.Td>
      <Table.Td>
        <Text size="sm" c="dimmed" style={{ wordBreak: 'break-word' }}>
          {probe.detail}
        </Text>
      </Table.Td>
    </Table.Tr>
  ));

  return (
    <Table striped highlightOnHover withTableBorder>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>{t('connectivity.column.service')}</Table.Th>
          <Table.Th>{t('connectivity.column.status')}</Table.Th>
          <Table.Th>{t('connectivity.column.latency')}</Table.Th>
          <Table.Th>{t('connectivity.column.detail')}</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>{rows}</Table.Tbody>
    </Table>
  );
}
