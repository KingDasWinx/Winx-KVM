import { useCallback, useEffect, useState } from 'react';
import { Button, Stack, Text } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';

import MonitorLayoutModal from '../shared/MonitorLayoutModal';
import { buildDefaultLayout } from '../../lib/monitorLayoutGeometry';
import * as ipc from '../../ipc/commands';
import type { MonitorLayoutDto } from '../../ipc/commands';

interface Props {
  workspaceId: string;
}

export default function WorkspaceLayoutEditor({ workspaceId }: Props) {
  const { t } = useTranslation('workspace');
  const [open, setOpen] = useState(false);
  const [layout, setLayout] = useState<MonitorLayoutDto | null>(null);
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [remoteUsername, setRemoteUsername] = useState<string | undefined>();
  const [loading, setLoading] = useState(false);
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

      const remoteMember = members.find((m) => m.device_id !== deviceInfo.id);
      if (!remoteMember) {
        setLayout(null);
        return;
      }

      setRemoteUsername(remoteMember.username);

      const remoteMonitors = await ipc.getPeerMonitors(
        remoteMember.device_id,
        workspaceId,
      );

      const existing = layoutDto.per_device[deviceInfo.id];
      if (existing) {
        setLayout({
          ...existing,
          local_monitors: localMonitors,
          remote_monitors: remoteMonitors.length > 0
            ? remoteMonitors
            : existing.remote_monitors ?? [],
        });
        return;
      }

      setLayout(buildDefaultLayout(
        localMonitors,
        remoteMember.device_id,
        remoteMonitors,
      ));
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
    if (open) void loadLayout();
  }, [open, loadLayout]);

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
      setOpen(false);
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

  return (
    <Stack gap="sm">
      <Text size="sm" fw={600}>{t('layoutEditor.title')}</Text>
      <Text size="xs" c="dimmed">{t('layoutEditor.hint')}</Text>
      <Button size="sm" variant="light" color="teal" onClick={() => setOpen(true)}>
        {t('layoutEditor.openModalButton')}
      </Button>

      <MonitorLayoutModal
        opened={open}
        onClose={() => setOpen(false)}
        layout={loading ? null : layout}
        onLayoutChange={setLayout}
        onSave={handleSave}
        saving={saving}
        loading={loading}
        remoteLabel={remoteUsername}
      />
    </Stack>
  );
}
