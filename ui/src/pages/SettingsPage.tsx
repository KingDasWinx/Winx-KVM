import { useCallback, useEffect, useState } from 'react';
import {
  Button,
  Code,
  Group,
  MultiSelect,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
  Tooltip,
} from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  getClipboardAutoSync,
  getDeviceInfo,
  getDiscoveryInterfaces,
  listNetworkInterfaces,
  setClipboardAutoSync,
  setDiscoveryInterfaces,
  updateDeviceUsername,
  type NetworkInterface,
} from '../ipc/commands';
import { SUPPORTED_LOCALES, type SupportedLocale } from '../i18n';

export function SettingsPage() {
  const { t, i18n } = useTranslation('settings');
  const [username, setUsername] = useState('');
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [clipboardAutoSync, setClipboardAutoSyncState] = useState(true);
  const [networkInterfaces, setNetworkInterfaces] = useState<NetworkInterface[]>([]);
  const [selectedInterfaces, setSelectedInterfaces] = useState<string[]>([]);
  const [savingInterfaces, setSavingInterfaces] = useState(false);

  const loadDevice = useCallback(() => {
    getDeviceInfo()
      .then((d) => setUsername(d.username))
      .catch((err: unknown) => {
        console.error('get_device_info failed', err);
      });
  }, []);

  const loadNetworkInterfaces = useCallback(() => {
    listNetworkInterfaces()
      .then(setNetworkInterfaces)
      .catch((err: unknown) => {
        console.error('list_network_interfaces failed', err);
      });
  }, []);

  const loadDiscoveryInterfaces = useCallback(() => {
    getDiscoveryInterfaces()
      .then(setSelectedInterfaces)
      .catch((err: unknown) => {
        console.error('get_discovery_interfaces failed', err);
      });
  }, []);

  useEffect(() => {
    loadDevice();
    loadNetworkInterfaces();
    loadDiscoveryInterfaces();
    getClipboardAutoSync()
      .then(setClipboardAutoSyncState)
      .catch((err: unknown) => {
        console.error('get_clipboard_auto_sync failed', err);
      });
  }, [loadDevice, loadNetworkInterfaces, loadDiscoveryInterfaces]);

  const handleSaveUsername = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const device = await updateDeviceUsername(username);
      setUsername(device.username);
    } catch (err: unknown) {
      const code = typeof err === 'string' ? err : 'general.internal_error';
      setSaveError(t(`error.${code}`, { ns: 'common', defaultValue: code }));
    } finally {
      setSaving(false);
    }
  };

  const localeOptions = SUPPORTED_LOCALES.map((lng) => ({
    value: lng,
    label: t(`language.${lng === 'en' ? 'en' : 'pt-BR'}`),
  }));

  return (
    <Stack gap="lg" maw={560}>
      <Title order={2}>{t('title')}</Title>

      <Stack gap="sm">
        <Title order={3}>{t('device.section_title')}</Title>
        <TextInput
          label={t('device.username_label')}
          description={t('device.username_hint')}
          value={username}
          onChange={(e) => setUsername(e.currentTarget.value)}
          maxLength={64}
        />
        {saveError && (
          <Text size="sm" c="red">
            {saveError}
          </Text>
        )}
        <Group>
          <Button loading={saving} onClick={() => void handleSaveUsername()}>
            {t('device.save')}
          </Button>
        </Group>
      </Stack>

      <Stack gap="sm">
        <Title order={3}>{t('language.section_title')}</Title>
        <Select
          label={t('language.label')}
          data={localeOptions}
          value={i18n.language}
          onChange={(value) => {
            if (value && SUPPORTED_LOCALES.includes(value as SupportedLocale)) {
              void i18n.changeLanguage(value);
            }
          }}
        />
      </Stack>

      <Stack gap="sm">
        <Title order={3}>{t('discovery.title')}</Title>
        <Text size="sm" c="dimmed">
          {t('discovery.description')}
        </Text>
        <MultiSelect
          label={t('discovery.interfaces.label')}
          placeholder={t('discovery.interfaces.placeholder')}
          description={t('discovery.interfaces.helper')}
          data={networkInterfaces.map((iface) => ({
            value: iface.name,
            label: iface.ipv4 ? `${iface.name} — ${iface.ipv4}` : iface.name,
          }))}
          value={selectedInterfaces}
          onChange={(value) => {
            setSelectedInterfaces(value);
            setSavingInterfaces(true);
            setDiscoveryInterfaces(value)
              .catch((err: unknown) => {
                console.error('set_discovery_interfaces failed', err);
              })
              .finally(() => {
                setSavingInterfaces(false);
              });
          }}
          disabled={savingInterfaces}
          searchable
          clearable
        />
      </Stack>

      <Stack gap="sm">
        <Title order={3}>{t('clipboard.section_title')}</Title>
        <Tooltip label={t('clipboard.auto_sync_hint')} withArrow>
          <Switch
            label={t('clipboard.auto_sync_label')}
            checked={clipboardAutoSync}
            onChange={(e) => {
              const checked = e.currentTarget.checked;
              setClipboardAutoSyncState(checked);
              setClipboardAutoSync(checked).catch((err: unknown) => {
                console.error('set_clipboard_auto_sync failed', err);
              });
            }}
          />
        </Tooltip>
      </Stack>

      <Stack gap="sm">
        <Title order={3}>{t('hotkeys.title')}</Title>
        <Text size="sm">
          {t('hotkeys.panic')}: <Code>{t('hotkeys.panic_keys')}</Code>
        </Text>
        <Text size="sm">
          {t('hotkeys.lock')}: <Code>{t('hotkeys.lock_keys')}</Code>
        </Text>
        <Text size="xs" c="dimmed">
          {t('hotkeys.readonly_note')}
        </Text>
      </Stack>
    </Stack>
  );
}
