import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Box, Button, Group, Modal, ScrollArea, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  computeCanvasScale,
  formatResolution,
  placedRemoteMonitors,
  withInferredEdges,
} from '../../lib/monitorLayoutGeometry';
import type { MonitorLayoutDto, MonitorRectDto } from '../../ipc/commands';
import classes from './monitorLayout.module.css';

const GRID_STEP = 40;

interface Props {
  opened: boolean;
  onClose: () => void;
  layout: MonitorLayoutDto | null;
  onLayoutChange: (layout: MonitorLayoutDto) => void;
  onSave: (layout: MonitorLayoutDto) => void | Promise<void>;
  saving?: boolean;
  loading?: boolean;
  remoteLabel?: string;
}

function toCanvas(rect: MonitorRectDto, scale: number) {
  return {
    left: rect.x * scale,
    top: rect.y * scale,
    width: rect.width * scale,
    height: rect.height * scale,
  };
}

function snap(value: number) {
  return Math.round(value / GRID_STEP) * GRID_STEP;
}

export default function MonitorLayoutModal({
  opened,
  onClose,
  layout,
  onLayoutChange,
  onSave,
  saving = false,
  loading = false,
  remoteLabel,
}: Props) {
  const { t } = useTranslation('workspace');
  const [dragging, setDragging] = useState(false);
  const dragOffset = useRef({ x: 0, y: 0 });
  const viewportRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ w: 900, h: 520 });

  useEffect(() => {
    if (!opened || !viewportRef.current) return;
    const el = viewportRef.current;
    const ro = new ResizeObserver(() => {
      setViewport({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    setViewport({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, [opened]);

  const scale = useMemo(() => {
    if (!layout) return 0.12;
    return computeCanvasScale(layout, viewport.w - 32, viewport.h - 32);
  }, [layout, viewport]);

  const placedRemote = useMemo(
    () => (layout ? placedRemoteMonitors(layout) : []),
    [layout],
  );

  const canvasSize = useMemo(() => {
    if (!layout) return { w: 400, h: 300 };
    const all = [...layout.local_monitors, ...placedRemote];
    const maxR = Math.max(...all.map((m) => (m.x + m.width) * scale), 400);
    const maxB = Math.max(...all.map((m) => (m.y + m.height) * scale), 300);
    return { w: maxR + 48, h: maxB + 48 };
  }, [layout, placedRemote, scale]);

  const handleSave = () => {
    if (!layout) return;
    const normalized = withInferredEdges(layout);
    onLayoutChange(normalized);
    void Promise.resolve(onSave(normalized));
  };

  const onRemotePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!layout) return;
    const canvas = event.currentTarget.closest(`.${classes.monitorCanvas}`);
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const bounds = placedRemoteMonitors(layout)[0] ?? layout.remote_virtual;
    const box = toCanvas(bounds, scale);

    dragOffset.current = {
      x: event.clientX - rect.left - box.left,
      y: event.clientY - rect.top - box.top,
    };
    setDragging(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onCanvasPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging || !layout) return;

      const rect = event.currentTarget.getBoundingClientRect();
      const canvasX = snap(event.clientX - rect.left - dragOffset.current.x);
      const canvasY = snap(event.clientY - rect.top - dragOffset.current.y);

      onLayoutChange({
        ...layout,
        remote_virtual: {
          ...layout.remote_virtual,
          x: Math.round(canvasX / scale),
          y: Math.round(canvasY / scale),
        },
      });
    },
    [dragging, layout, onLayoutChange, scale],
  );

  const onCanvasPointerUp = () => {
    if (dragging && layout) {
      onLayoutChange(withInferredEdges(layout));
    }
    setDragging(false);
  };

  const exitId = layout?.edge.exit_local_monitor_id;
  const edgeLabel = layout
    ? `${layout.edge.local_exit} → ${layout.edge.remote_entry}`
    : '';

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={t('layoutEditor.modalTitle')}
      size="90%"
      centered
      overlayProps={{ backgroundOpacity: 0.65, blur: 4 }}
      styles={{
        content: { background: '#111820' },
        header: { background: '#111820', borderBottom: '1px solid rgba(255,255,255,0.06)' },
        title: { color: '#e8edf4', fontWeight: 600, letterSpacing: '0.02em' },
      }}
    >
      <Stack gap="md">
        <Text size="sm" c="dimmed">{t('layoutEditor.modalHint')}</Text>

        {loading && (
          <Text size="sm" c="dimmed">{t('layoutEditor.loading')}</Text>
        )}
        {!loading && !layout && (
          <Text size="sm" c="dimmed">{t('layoutEditor.noRemoteMember')}</Text>
        )}
        {!loading && layout && (
          <>
            <Group gap="md">
              <Text className={classes.edgeBadge}>{edgeLabel}</Text>
              {exitId != null && (
                <Text size="xs" c="dimmed">
                  {t('layoutEditor.exitMonitor', { id: exitId })}
                </Text>
              )}
            </Group>

            <ScrollArea.Autosize mah="62vh" offsetScrollbars>
              <Box ref={viewportRef} style={{ minHeight: 420 }}>
                <Box
                  className={classes.monitorCanvas}
                  onPointerMove={onCanvasPointerMove}
                  onPointerUp={onCanvasPointerUp}
                  onPointerLeave={onCanvasPointerUp}
                  style={{ width: canvasSize.w, height: canvasSize.h, margin: '0 auto' }}
                >
                  {layout.local_monitors.map((monitor, idx) => {
                    const box = toCanvas(monitor, scale);
                    const isExit = monitor.id === exitId;
                    return (
                      <Box
                        key={monitor.id}
                        className={`${classes.localMonitor} ${isExit ? classes.localMonitorExit : ''}`}
                        style={{
                          left: box.left,
                          top: box.top,
                          width: box.width,
                          height: box.height,
                        }}
                      >
                        <span>{t('layoutEditor.localLabel')} {idx + 1}</span>
                        <span className={classes.resBadge}>{formatResolution(monitor)}</span>
                        {isExit && (
                          <span className={classes.resBadge}>{t('layoutEditor.crossingEdge')}</span>
                        )}
                      </Box>
                    );
                  })}

                  {placedRemote.map((monitor, idx) => {
                    const box = toCanvas(monitor, scale);
                    return (
                      <Box
                        key={`remote-${monitor.id}-${idx}`}
                        className={`${classes.remoteMonitor} ${dragging ? classes.remoteMonitorDragging : ''}`}
                        style={{
                          left: box.left,
                          top: box.top,
                          width: box.width,
                          height: box.height,
                        }}
                        onPointerDown={onRemotePointerDown}
                      >
                        <span>{remoteLabel ?? t('layoutEditor.remoteLabel')} {idx + 1}</span>
                        <span className={classes.resBadge}>{formatResolution(monitor)}</span>
                      </Box>
                    );
                  })}
                </Box>
              </Box>
            </ScrollArea.Autosize>

            <Box className={classes.legendRow}>
              <span>
                <span className={`${classes.legendDot} ${classes.legendLocal}`} />
                {t('layoutEditor.legendLocal')}
              </span>
              <span>
                <span className={`${classes.legendDot} ${classes.legendRemote}`} />
                {t('layoutEditor.legendRemote')}
              </span>
              <span>
                <span className={`${classes.legendDot} ${classes.legendExit}`} />
                {t('layoutEditor.legendCrossing')}
              </span>
            </Box>
          </>
        )}

        <Group justify="flex-end">
          <Button variant="subtle" color="gray" onClick={onClose}>
            {t('layoutEditor.cancelButton')}
          </Button>
          <Button onClick={handleSave} loading={saving} color="teal" disabled={!layout}>
            {t('layoutEditor.saveButton')}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
