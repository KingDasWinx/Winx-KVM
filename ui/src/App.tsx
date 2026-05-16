import { useEffect, useState } from 'react';
import {
  AppShell,
  Badge,
  Button,
  Code,
  Group,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { getAppInfo, ping, type AppInfo } from './ipc/commands';
import { onWinxEvent } from './ipc/events';

export function App() {
  const { t } = useTranslation('common');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [pong, setPong] = useState<string>('');
  const [eventCount, setEventCount] = useState(0);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch((err) => console.error('app_info failed', err));

    const unlisten = onWinxEvent(() => {
      setEventCount((n) => n + 1);
    });

    return () => {
      unlisten.then((u) => u());
    };
  }, []);

  return (
    <AppShell padding="md">
      <AppShell.Main>
        <Stack gap="md" maw={720} mx="auto" mt="xl">
          <Group justify="space-between" align="baseline">
            <Title order={1}>{t('app.name')}</Title>
            <Badge color="yellow" variant="light">
              {t('app.status_planning')}
            </Badge>
          </Group>

          <Text c="dimmed">{t('app.tagline')}</Text>

          {info ? (
            <Stack gap="xs">
              <Text size="sm">
                <strong>{t('app.version_label')}:</strong>{' '}
                <Code>{info.version}</Code>
              </Text>
              <Text size="sm">
                <strong>{t('app.protocol_label')}:</strong>{' '}
                <Code>v{info.protocol_version}</Code>
              </Text>
            </Stack>
          ) : (
            <Text size="sm" c="dimmed">
              {t('app.loading')}
            </Text>
          )}

          <Group>
            <Button
              onClick={async () => {
                const result = await ping('hello from React');
                setPong(result);
              }}
            >
              {t('app.ping_button')}
            </Button>
            {pong && <Code>{pong}</Code>}
          </Group>

          <Text size="xs" c="dimmed">
            {t('app.events_received', { count: eventCount })}
          </Text>
        </Stack>
      </AppShell.Main>
    </AppShell>
  );
}
