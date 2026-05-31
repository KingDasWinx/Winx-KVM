import { useCallback, useEffect, useState } from 'react';
import { Button, Stack } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';

import MonitorLayoutModal from '../shared/MonitorLayoutModal';
import { buildDefaultSessionLayout } from '../../lib/monitorLayoutGeometry';
import * as ipc from '../../ipc/commands';
import type { SessionDesktopLayoutDto } from '../../ipc/commands';

interface Props {
  peerId: string;
  peerUsername?: string;
}

export default function SingleKvmLayoutPanel({ peerId, peerUsername }: Props) {
  const { t } = useTranslation('common');
  const { t: tw } = useTranslation('workspace');
  const [open, setOpen] = useState(false);
  const [sessionLayout, setSessionLayout] = useState<SessionDesktopLayoutDto | null>(null);
  const [localDeviceId, setLocalDeviceId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const loadLayout = useCallback(async () => {
    setLoading(true);
    try {
      const [deviceInfo, saved, remoteMonitors, localMonitors] = await Promise.all([
        ipc.getDeviceInfo(),
        ipc.getKvmSessionLayout(peerId),
        ipc.getPeerMonitors(peerId),
        ipc.listLocalMonitors(),
      ]);

      setLocalDeviceId(deviceInfo.id);

      if (saved) {
        setSessionLayout(saved);
      } else {
        setSessionLayout(
          buildDefaultSessionLayout(
            deviceInfo.id,
            localMonitors,
            peerId,
            remoteMonitors,
          ),
        );
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
    if (open) void loadLayout();
  }, [open, loadLayout]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<{ kind: string; peer_id?: string }>('domain-event', (event) => {
      const { kind, peer_id: eventPeerId } = event.payload;
      if (eventPeerId !== peerId) return;
      if (kind !== 'peer-monitors-updated' && kind !== 'kvm-layout-updated') return;
      void loadLayout();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [peerId, loadLayout]);

  const handleSave = async (layout: SessionDesktopLayoutDto) => {
    setSaving(true);
    try {
      await ipc.updateKvmSessionLayout({ peerId, layout });
      notifications.show({
        title: tw('layoutEditor.saveSuccessTitle'),
        message: tw('layoutEditor.saveSuccessMessage'),
        color: 'green',
      });
      setOpen(false);
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
      <Button
        size="xs"
        variant="light"
        color="teal"
        onClick={() => setOpen(true)}
      >
        {t('kvm.layout_panel_title')}
      </Button>

      <MonitorLayoutModal
        opened={open}
        onClose={() => setOpen(false)}
        sessionLayout={loading ? null : sessionLayout}
        localDeviceId={localDeviceId ?? undefined}
        remoteDeviceId={peerId}
        onSessionLayoutChange={setSessionLayout}
        onSaveSession={handleSave}
        saving={saving}
        loading={loading}
        remoteLabel={peerUsername}
      />
    </Stack>
  );
}
