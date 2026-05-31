import { useCallback, useEffect, useRef, useState } from 'react';
import { Box, Button, Group, Stack, Text } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useTranslation } from 'react-i18next';

import * as ipc from '../../ipc/commands';
import type { MonitorLayoutDto, MonitorRectDto } from '../../ipc/commands';

const CANVAS_SCALE = 0.12;
const GRID_STEP = 40;
const REMOTE_MONITOR_ID = 65535;

interface Props {
  workspaceId: string;
}

function toCanvas(rect: MonitorRectDto) {
  return {
    left: rect.x * CANVAS_SCALE,
    top: rect.y * CANVAS_SCALE,
    width: rect.width * CANVAS_SCALE,
    height: rect.height * CANVAS_SCALE,
  };
}

function snap(value: number) {
  return Math.round(value / GRID_STEP) * GRID_STEP;
}

function defaultLayout(remotePeerId: string): MonitorLayoutDto {
  return {
    local_monitors: [{ id: 1, x: 0, y: 0, width: 1920, height: 1080 }],
    remote_peer: remotePeerId,
    remote_virtual: { id: REMOTE_MONITOR_ID, x: 1920, y: 0, width: 1920, height: 1080 },
    edge: { local_exit: 'Right', remote_entry: 'Left' },
  };
}

export default function WorkspaceLayoutEditor({ workspaceId }: Props) {
  const { t } = useTranslation('workspace');
  const [layout, setLayout] = useState<MonitorLayoutDto | null>(null);
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dragging, setDragging] = useState(false);
  const dragOffset = useRef({ x: 0, y: 0 });

  const loadLayout = useCallback(async () => {
    setLoading(true);
    try {
      const [deviceInfo, layoutDto, members] = await Promise.all([
        ipc.getDeviceInfo(),
        ipc.getWorkspaceLayout(workspaceId),
        ipc.listWorkspaceMembers(workspaceId),
      ]);

      setDeviceId(deviceInfo.id);

      const existing = layoutDto.per_device[deviceInfo.id];
      if (existing) {
        setLayout(existing);
        return;
      }

      const remoteMember = members.find((m) => m.device_id !== deviceInfo.id);
      if (!remoteMember) {
        setLayout(null);
        return;
      }

      setLayout(defaultLayout(remoteMember.device_id));
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

  const handleSave = async () => {
    if (!layout || !deviceId) return;

    setSaving(true);
    try {
      await ipc.updateWorkspaceLayout({ workspaceId, deviceId, layout });
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

  const onRemotePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!layout) return;
    const canvas = event.currentTarget.parentElement;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const canvasX = event.clientX - rect.left;
    const canvasY = event.clientY - rect.top;
    const remote = toCanvas(layout.remote_virtual);

    dragOffset.current = {
      x: canvasX - remote.left,
      y: canvasY - remote.top,
    };
    setDragging(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onCanvasPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging || !layout) return;

    const rect = event.currentTarget.getBoundingClientRect();
    const canvasX = snap(event.clientX - rect.left - dragOffset.current.x);
    const canvasY = snap(event.clientY - rect.top - dragOffset.current.y);

    setLayout({
      ...layout,
      remote_virtual: {
        ...layout.remote_virtual,
        x: Math.round(canvasX / CANVAS_SCALE),
        y: Math.round(canvasY / CANVAS_SCALE),
      },
    });
  };

  const onCanvasPointerUp = () => {
    setDragging(false);
  };

  if (loading) {
    return <Text size="sm" c="dimmed">{t('layoutEditor.loading')}</Text>;
  }

  if (!layout) {
    return <Text size="sm" c="dimmed">{t('layoutEditor.noRemoteMember')}</Text>;
  }

  const canvasWidth =
    Math.max(
      ...layout.local_monitors.map((m) => (m.x + m.width) * CANVAS_SCALE),
      (layout.remote_virtual.x + layout.remote_virtual.width) * CANVAS_SCALE,
    ) + 40;
  const canvasHeight =
    Math.max(
      ...layout.local_monitors.map((m) => (m.y + m.height) * CANVAS_SCALE),
      (layout.remote_virtual.y + layout.remote_virtual.height) * CANVAS_SCALE,
    ) + 40;

  const remoteBox = toCanvas(layout.remote_virtual);

  return (
    <Stack gap="sm">
      <Text size="sm" fw={600}>{t('layoutEditor.title')}</Text>
      <Text size="xs" c="dimmed">{t('layoutEditor.hint')}</Text>

      <Box
        onPointerMove={onCanvasPointerMove}
        onPointerUp={onCanvasPointerUp}
        onPointerLeave={onCanvasPointerUp}
        style={{
          position: 'relative',
          width: canvasWidth,
          height: canvasHeight,
          border: '1px solid var(--mantine-color-gray-4)',
          borderRadius: 8,
          backgroundImage:
            'linear-gradient(to right, var(--mantine-color-gray-1) 1px, transparent 1px), linear-gradient(to bottom, var(--mantine-color-gray-1) 1px, transparent 1px)',
          backgroundSize: `${GRID_STEP}px ${GRID_STEP}px`,
          touchAction: 'none',
        }}
      >
        {layout.local_monitors.map((monitor) => {
          const box = toCanvas(monitor);
          return (
            <Box
              key={monitor.id}
              style={{
                position: 'absolute',
                left: box.left,
                top: box.top,
                width: box.width,
                height: box.height,
                border: '2px solid var(--mantine-color-blue-5)',
                borderRadius: 4,
                background: 'var(--mantine-color-blue-0)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: 11,
                color: 'var(--mantine-color-blue-7)',
                userSelect: 'none',
              }}
            >
              {t('layoutEditor.localLabel')}
            </Box>
          );
        })}

        <Box
          onPointerDown={onRemotePointerDown}
          style={{
            position: 'absolute',
            left: remoteBox.left,
            top: remoteBox.top,
            width: remoteBox.width,
            height: remoteBox.height,
            border: '2px solid var(--mantine-color-teal-5)',
            borderRadius: 4,
            background: 'var(--mantine-color-teal-0)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 11,
            color: 'var(--mantine-color-teal-7)',
            cursor: dragging ? 'grabbing' : 'grab',
            userSelect: 'none',
          }}
        >
          {t('layoutEditor.remoteLabel')}
        </Box>
      </Box>

      <Group justify="flex-end">
        <Button size="xs" onClick={handleSave} loading={saving}>
          {t('layoutEditor.saveButton')}
        </Button>
      </Group>
    </Stack>
  );
}
