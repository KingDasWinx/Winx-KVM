import { useCallback, useEffect, useState } from 'react';
import {
  Button,
  Code,
  Group,
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
  setClipboardAutoSync,
  updateDeviceUsername,
} from '../ipc/commands';
import { SUPPORTED_LOCALES, type SupportedLocale } from '../i18n';

export function SettingsPage() {
  const { t, i18n } = useTranslation('settings');
  const [username, setUsername] = useState('');
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [clipboardAutoSync, setClipboardAutoSyncState] = useState(true);

  const loadDevice = useCallback(() => {
    getDeviceInfo()
      .then((d) => setUsername(d.username))
      .catch((err: unknown) => {
        console.error('get_device_info failed', err);
      });
  }, []);

  useEffect(() => {
    loadDevice();
    getClipboardAutoSync()
      .then(setClipboardAutoSyncState)
      .catch((err: unknown) => {
        console.error('get_clipboard_auto_sync failed', err);
      });
  }, [loadDevice]);

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
