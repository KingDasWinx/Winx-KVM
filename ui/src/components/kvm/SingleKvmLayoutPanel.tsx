import { useCallback, useEffect, useState } from 'react';
import { Button, Stack } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';

import MonitorLayoutModal from '../shared/MonitorLayoutModal';
import { buildDefaultLayout } from '../../lib/monitorLayoutGeometry';
import * as ipc from '../../ipc/commands';
import type { MonitorLayoutDto } from '../../ipc/commands';

interface Props {
  peerId: string;
  peerUsername?: string;
}

export default function SingleKvmLayoutPanel({ peerId, peerUsername }: Props) {
  const { t } = useTranslation('common');
  const { t: tw } = useTranslation('workspace');
  const [open, setOpen] = useState(false);
  const [layout, setLayout] = useState<MonitorLayoutDto | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const loadLayout = useCallback(async () => {
    setLoading(true);
    try {
      const [localMonitors, saved, remoteMonitors] = await Promise.all([
        ipc.listLocalMonitors(),
        ipc.getKvmLayout(peerId),
        ipc.getPeerMonitors(peerId),
      ]);

      if (saved) {
        setLayout({
          ...saved,
          local_monitors: localMonitors,
          remote_monitors: remoteMonitors.length > 0
            ? remoteMonitors
            : saved.remote_monitors,
        });
      } else {
        setLayout(buildDefaultLayout(localMonitors, peerId, remoteMonitors));
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

  const handleSave = async (normalized: MonitorLayoutDto) => {
    setSaving(true);
    try {
      await ipc.updateKvmLayout({ peerId, layout: normalized });
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
        layout={loading ? null : layout}
        onLayoutChange={setLayout}
        onSave={handleSave}
        saving={saving}
        loading={loading}
        remoteLabel={peerUsername}
      />
    </Stack>
  );
}
