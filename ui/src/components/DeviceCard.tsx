import { useEffect, useState } from 'react';
import { Badge, Card, Group, Skeleton, Stack, Text, Tooltip } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router-dom';

import { getDeviceInfo, type DeviceInfo } from '../ipc/commands';

export function DeviceCard() {
  const { t } = useTranslation('common');
  const { pathname } = useLocation();
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setError(null);
    getDeviceInfo()
      .then(setDevice)
      .catch((err: unknown) => {
        const code = typeof err === 'string' ? err : 'general.internal_error';
        setError(t(`error.${code}`, { defaultValue: code }));
      });
  }, [t, pathname]);

  if (error) {
    return (
      <Card withBorder radius="md" p="md">
        <Text c="red" size="sm">{error}</Text>
      </Card>
    );
  }

  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="xs">
        <Group justify="space-between" align="center">
          <Text fw={600} size="sm">{t('device.this_device')}</Text>
          <Badge color="green" variant="light" size="sm">
            {t('device.status_local')}
          </Badge>
        </Group>

        {device ? (
          <>
            <Text size="lg" fw={700}>{device.username}</Text>
            <Tooltip label={t('device.fingerprint_hint')} withArrow>
              <Text
                size="xs"
                c="dimmed"
                ff="monospace"
                style={{ cursor: 'default', letterSpacing: '0.05em' }}
              >
                {device.fingerprint}
              </Text>
            </Tooltip>
          </>
        ) : (
          <>
            <Skeleton height={28} width={160} radius="sm" />
            <Skeleton height={16} width={230} radius="sm" />
          </>
        )}
      </Stack>
    </Card>
  );
}
