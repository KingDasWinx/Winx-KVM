import { useCallback, useEffect, useState } from 'react';
import { Stack, Text, UnstyledButton } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';

import MonitorLayoutEditor from '../shared/MonitorLayoutEditor';
import { buildDefaultLayout } from '../../lib/monitorLayoutGeometry';
import * as ipc from '../../ipc/commands';
import type { MonitorLayoutDto } from '../../ipc/commands';

interface Props {
  peerId: string;
  defaultOpen?: boolean;
}

export default function SingleKvmLayoutPanel({ peerId, defaultOpen = false }: Props) {
  const { t } = useTranslation('common');
  const { t: tw } = useTranslation('workspace');
  const [open, setOpen] = useState(defaultOpen);
  const [layout, setLayout] = useState<MonitorLayoutDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const loadLayout = useCallback(async () => {
    setLoading(true);
    try {
      const localMonitors = await ipc.listLocalMonitors();
      const saved = await ipc.getKvmLayout(peerId);
      if (saved) {
        setLayout({ ...saved, local_monitors: localMonitors });
      } else {
        setLayout(buildDefaultLayout(localMonitors, peerId));
      }
    } catch (err) {
      console.error('Failed to load KVM layout:', err);
      notifications.show({
        title: tw('layoutEditor.loadErrorTitle'),
        message: tw('layoutEditor.loadErrorMessage'),
        color: 'red',
      });
    } finally {
      setLoading(false);
    }
  }, [peerId, tw]);

  useEffect(() => {
    void loadLayout();
  }, [loadLayout]);

  const handleSave = async (normalized: MonitorLayoutDto) => {
    setSaving(true);
    try {
      await ipc.updateKvmLayout({ peerId, layout: normalized });
      notifications.show({
        title: tw('layoutEditor.saveSuccessTitle'),
        message: tw('layoutEditor.saveSuccessMessage'),
        color: 'green',
      });
    } catch (err) {
      console.error('Failed to save KVM layout:', err);
      notifications.show({
        title: tw('layoutEditor.saveErrorTitle'),
        message: tw('layoutEditor.saveErrorMessage'),
        color: 'red',
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Stack gap="xs">
      <UnstyledButton onClick={() => setOpen((v) => !v)}>
        <Text size="sm" fw={500} c="blue">
          {open ? '▼' : '▶'} {t('kvm.layout_panel_title')}
        </Text>
      </UnstyledButton>
      {open && (
        loading ? (
          <Text size="sm" c="dimmed">{tw('layoutEditor.loading')}</Text>
        ) : layout ? (
          <MonitorLayoutEditor
            layout={layout}
            onLayoutChange={setLayout}
            onSave={handleSave}
            saving={saving}
          />
        ) : null
      )}
    </Stack>
  );
}
