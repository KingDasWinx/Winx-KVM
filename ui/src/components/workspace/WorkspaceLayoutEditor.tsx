import { useCallback, useEffect, useState } from 'react';
import { Stack, Text } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';

import MonitorLayoutEditor from '../shared/MonitorLayoutEditor';
import { buildDefaultLayout } from '../../lib/monitorLayoutGeometry';
import * as ipc from '../../ipc/commands';
import type { MonitorLayoutDto } from '../../ipc/commands';

interface Props {
  workspaceId: string;
}

export default function WorkspaceLayoutEditor({ workspaceId }: Props) {
  const { t } = useTranslation('workspace');
  const [layout, setLayout] = useState<MonitorLayoutDto | null>(null);
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const loadLayout = useCallback(async () => {
    setLoading(true);
    try {
      const [deviceInfo, layoutDto, members, localMonitors] = await Promise.all([
        ipc.getDeviceInfo(),
        ipc.getWorkspaceLayout(workspaceId),
        ipc.listWorkspaceMembers(workspaceId),
        ipc.listLocalMonitors(),
      ]);

      setDeviceId(deviceInfo.id);

      const existing = layoutDto.per_device[deviceInfo.id];
      const remoteMember = members.find((m) => m.device_id !== deviceInfo.id);
      if (!remoteMember) {
        setLayout(null);
        return;
      }

      if (existing) {
        setLayout({ ...existing, local_monitors: localMonitors });
        return;
      }

      setLayout(buildDefaultLayout(localMonitors, remoteMember.device_id));
    } catch (err) {
      console.error('Failed to load workspace layout:', err);
      notifications.show({
        title: t('layoutEditor.loadErrorTitle'),
        message: t('layoutEditor.loadErrorMessage'),
        color: 'red',
      });
    } finally {
      setLoading(false);
    }
  }, [t, workspaceId]);

  useEffect(() => {
    void loadLayout();
  }, [loadLayout]);

  const handleSave = async (normalized: MonitorLayoutDto) => {
    if (!deviceId) return;

    setSaving(true);
    try {
      await ipc.updateWorkspaceLayout({ workspaceId, deviceId, layout: normalized });
      notifications.show({
        title: t('layoutEditor.saveSuccessTitle'),
        message: t('layoutEditor.saveSuccessMessage'),
        color: 'green',
      });
    } catch (err) {
      console.error('Failed to save workspace layout:', err);
      notifications.show({
        title: t('layoutEditor.saveErrorTitle'),
        message: t('layoutEditor.saveErrorMessage'),
        color: 'red',
      });
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return <Text size="sm" c="dimmed">{t('layoutEditor.loading')}</Text>;
  }

  if (!layout) {
    return <Text size="sm" c="dimmed">{t('layoutEditor.noRemoteMember')}</Text>;
  }

  return (
    <Stack gap="sm">
      <MonitorLayoutEditor
        layout={layout}
        onLayoutChange={setLayout}
        onSave={handleSave}
        saving={saving}
      />
    </Stack>
  );
}
