import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ActionIcon, Box, Button, Group, Modal, Stack, Text, Tooltip } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import {
  computeWorldFrameFromSession,
  deriveRuntimeForLocalDevice,
  formatResolution,
  moveDeviceMonitors,
  placedRemoteMonitors,
  sessionAllMonitors,
  withInferredEdges,
} from '../../lib/monitorLayoutGeometry';
import type {
  MonitorLayoutDto,
  MonitorRectDto,
  SessionDesktopLayoutDto,
} from '../../ipc/commands';
import classes from './monitorLayout.module.css';

/** Padding em pixels virtuais (desktop) ao redor do conteúdo — mundo fixo ao abrir. */
const WORLD_PAD = 2400;
const DISPLAY_SCALE = 0.12;
const GRID_STEP = 40;
const ZOOM_MIN = 0.08;
const ZOOM_MAX = 4;

interface Props {
  opened: boolean;
  onClose: () => void;
  /** Modo legado (workspace): perspectiva local. */
  layout?: MonitorLayoutDto | null;
  onLayoutChange?: (layout: MonitorLayoutDto) => void;
  onSave?: (layout: MonitorLayoutDto) => void | Promise<void>;
  /** Modo canônico (single KVM): mesmo layout em todos os PCs. */
  sessionLayout?: SessionDesktopLayoutDto | null;
  localDeviceId?: string;
  remoteDeviceId?: string;
  onSessionLayoutChange?: (layout: SessionDesktopLayoutDto) => void;
  onSaveSession?: (layout: SessionDesktopLayoutDto) => void | Promise<void>;
  saving?: boolean;
  loading?: boolean;
  remoteLabel?: string;
}

interface ViewTransform {
  panX: number;
  panY: number;
  zoom: number;
}

interface WorldShift {
  x: number;
  y: number;
}

function snap(value: number) {
  return Math.round(value / GRID_STEP) * GRID_STEP;
}

function toScreen(
  rect: MonitorRectDto,
  shift: WorldShift,
): { left: number; top: number; width: number; height: number } {
  return {
    left: (rect.x + shift.x) * DISPLAY_SCALE,
    top: (rect.y + shift.y) * DISPLAY_SCALE,
    width: rect.width * DISPLAY_SCALE,
    height: rect.height * DISPLAY_SCALE,
  };
}

function computeWorldFrame(layout: MonitorLayoutDto): {
  worldW: number;
  worldH: number;
  shift: WorldShift;
} {
  const placed = placedRemoteMonitors(layout);
  const all = [...layout.local_monitors, ...placed];
  if (all.length === 0) {
    const fallback = 4800 * DISPLAY_SCALE;
    return { worldW: fallback, worldH: fallback * 0.75, shift: { x: 0, y: 0 } };
  }
  const minX = Math.min(...all.map((m) => m.x));
  const minY = Math.min(...all.map((m) => m.y));
  const maxR = Math.max(...all.map((m) => m.x + m.width));
  const maxB = Math.max(...all.map((m) => m.y + m.height));
  const virtualW = maxR - minX + WORLD_PAD * 2;
  const virtualH = maxB - minY + WORLD_PAD * 2;
  return {
    worldW: virtualW * DISPLAY_SCALE,
    worldH: virtualH * DISPLAY_SCALE,
    shift: { x: WORLD_PAD - minX, y: WORLD_PAD - minY },
  };
}

function computeWorldShift(layout: MonitorLayoutDto): WorldShift {
  return computeWorldFrame(layout).shift;
}

function contentScreenBounds(
  layout: MonitorLayoutDto,
  shift: WorldShift,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const placed = placedRemoteMonitors(layout);
  const all = [...layout.local_monitors, ...placed];
  const xs = all.map((m) => (m.x + shift.x) * DISPLAY_SCALE);
  const ys = all.map((m) => (m.y + shift.y) * DISPLAY_SCALE);
  const rs = all.map((m) => (m.x + shift.x + m.width) * DISPLAY_SCALE);
  const bs = all.map((m) => (m.y + shift.y + m.height) * DISPLAY_SCALE);
  return {
    minX: Math.min(...xs),
    minY: Math.min(...ys),
    maxX: Math.max(...rs),
    maxY: Math.max(...bs),
  };
}

function contentScreenBoundsSession(
  session: SessionDesktopLayoutDto,
  shift: WorldShift,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const all = sessionAllMonitors(session).map((e) => e.monitor);
  const xs = all.map((m) => (m.x + shift.x) * DISPLAY_SCALE);
  const ys = all.map((m) => (m.y + shift.y) * DISPLAY_SCALE);
  const rs = all.map((m) => (m.x + shift.x + m.width) * DISPLAY_SCALE);
  const bs = all.map((m) => (m.y + shift.y + m.height) * DISPLAY_SCALE);
  return {
    minX: Math.min(...xs),
    minY: Math.min(...ys),
    maxX: Math.max(...rs),
    maxY: Math.max(...bs),
  };
}

function deviceMinBounds(monitors: MonitorRectDto[]): { minX: number; minY: number } {
  const first = monitors[0];
  if (!first) return { minX: 0, minY: 0 };
  return {
    minX: Math.min(...monitors.map((m) => m.x)),
    minY: Math.min(...monitors.map((m) => m.y)),
  };
}

export default function MonitorLayoutModal({
  opened,
  onClose,
  layout = null,
  onLayoutChange,
  onSave,
  sessionLayout = null,
  localDeviceId,
  remoteDeviceId,
  onSessionLayoutChange,
  onSaveSession,
  saving = false,
  loading = false,
  remoteLabel,
}: Props) {
  const { t } = useTranslation('workspace');
  const isSessionMode =
    sessionLayout != null && localDeviceId != null && remoteDeviceId != null;
  const hasContent = isSessionMode ? !!sessionLayout : !!layout;

  const viewportRef = useRef<HTMLDivElement>(null);
  const [worldFrame, setWorldFrame] = useState<{
    worldW: number;
    worldH: number;
    shift: WorldShift;
  } | null>(null);
  const [view, setView] = useState<ViewTransform>({ panX: 0, panY: 0, zoom: 0.15 });
  const [worldShift, setWorldShift] = useState<WorldShift>({ x: 0, y: 0 });
  const [draggingRemote, setDraggingRemote] = useState(false);
  const [draggingDevice, setDraggingDevice] = useState<string | null>(null);
  const [panning, setPanning] = useState(false);
  const dragOffset = useRef({ x: 0, y: 0 });
  const panStart = useRef({ x: 0, y: 0, panX: 0, panY: 0 });

  const canvasW = worldFrame?.worldW ?? 4800 * DISPLAY_SCALE;
  const canvasH = worldFrame?.worldH ?? 3600 * DISPLAY_SCALE;

  const placedRemote = useMemo(
    () => (layout ? placedRemoteMonitors(layout) : []),
    [layout],
  );

  const runtimeForBadge = useMemo(() => {
    if (!isSessionMode || !sessionLayout || !localDeviceId || !remoteDeviceId) {
      return layout;
    }
    return deriveRuntimeForLocalDevice(sessionLayout, localDeviceId, remoteDeviceId);
  }, [isSessionMode, sessionLayout, localDeviceId, remoteDeviceId, layout]);

  const fitToContent = useCallback(() => {
    if (!hasContent || !viewportRef.current) return;
    const shift = worldFrame?.shift ?? (isSessionMode && sessionLayout
      ? computeWorldFrameFromSession(sessionLayout).shift
      : layout
        ? computeWorldShift(layout)
        : { x: 0, y: 0 });
    setWorldShift(shift);
    const bounds = isSessionMode && sessionLayout
      ? contentScreenBoundsSession(sessionLayout, shift)
      : layout
        ? contentScreenBounds(layout, shift)
        : { minX: 0, minY: 0, maxX: 1, maxY: 1 };
    const contentW = bounds.maxX - bounds.minX;
    const contentH = bounds.maxY - bounds.minY;
    const pad = 64;
    const vw = viewportRef.current.clientWidth;
    const vh = viewportRef.current.clientHeight;
    const zoom = Math.min((vw - pad) / contentW, (vh - pad) / contentH);
    const cx = (bounds.minX + bounds.maxX) / 2;
    const cy = (bounds.minY + bounds.maxY) / 2;
    setView({
      zoom: Math.max(zoom, ZOOM_MIN),
      panX: vw / 2 - cx * zoom,
      panY: vh / 2 - cy * zoom,
    });
  }, [hasContent, isSessionMode, sessionLayout, layout, worldFrame]);

  useEffect(() => {
    if (!opened) {
      setWorldFrame(null);
      return;
    }
    if (hasContent && !loading && !worldFrame) {
      const frame = isSessionMode && sessionLayout
        ? computeWorldFrameFromSession(sessionLayout)
        : layout
          ? computeWorldFrame(layout)
          : null;
      if (frame) {
        setWorldFrame(frame);
        setWorldShift(frame.shift);
        requestAnimationFrame(() => fitToContent());
      }
    }
  }, [opened, hasContent, isSessionMode, sessionLayout, layout, loading, worldFrame, fitToContent]);

  const screenToWorld = useCallback(
    (clientX: number, clientY: number): { x: number; y: number } => {
      const rect = viewportRef.current?.getBoundingClientRect();
      if (!rect) return { x: 0, y: 0 };
      const localX = (clientX - rect.left - view.panX) / view.zoom;
      const localY = (clientY - rect.top - view.panY) / view.zoom;
      return {
        x: localX / DISPLAY_SCALE - worldShift.x,
        y: localY / DISPLAY_SCALE - worldShift.y,
      };
    },
    [view.panX, view.panY, view.zoom, worldShift],
  );

  const handleSave = () => {
    if (isSessionMode && sessionLayout && onSaveSession) {
      void Promise.resolve(onSaveSession(sessionLayout));
      return;
    }
    if (!layout || !onSave || !onLayoutChange) return;
    const normalized = withInferredEdges(layout);
    onLayoutChange(normalized);
    void Promise.resolve(onSave(normalized));
  };

  const onRemotePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!layout || !onLayoutChange) return;
    event.stopPropagation();
    const world = screenToWorld(event.clientX, event.clientY);
    dragOffset.current = {
      x: world.x - layout.remote_virtual.x,
      y: world.y - layout.remote_virtual.y,
    };
    setDraggingRemote(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onDevicePointerDown = (
    deviceId: string,
    event: React.PointerEvent<HTMLDivElement>,
  ) => {
    if (!sessionLayout || !onSessionLayoutChange) return;
    event.stopPropagation();
    const monitors = sessionLayout.per_device[deviceId] ?? [];
    const { minX, minY } = deviceMinBounds(monitors);
    const world = screenToWorld(event.clientX, event.clientY);
    dragOffset.current = { x: world.x - minX, y: world.y - minY };
    setDraggingDevice(deviceId);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onViewportPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    if (draggingRemote || draggingDevice) return;
    panStart.current = {
      x: event.clientX,
      y: event.clientY,
      panX: view.panX,
      panY: view.panY,
    };
    setPanning(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onViewportPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (draggingDevice && sessionLayout && onSessionLayoutChange) {
        const monitors = sessionLayout.per_device[draggingDevice] ?? [];
        const world = screenToWorld(event.clientX, event.clientY);
        const newMinX = snap(world.x - dragOffset.current.x);
        const newMinY = snap(world.y - dragOffset.current.y);
        const { minX, minY } = deviceMinBounds(monitors);
        onSessionLayoutChange(
          moveDeviceMonitors(
            sessionLayout,
            draggingDevice,
            newMinX - minX,
            newMinY - minY,
          ),
        );
        return;
      }
      if (draggingRemote && layout && onLayoutChange) {
        const world = screenToWorld(event.clientX, event.clientY);
        onLayoutChange({
          ...layout,
          remote_virtual: {
            ...layout.remote_virtual,
            x: snap(world.x - dragOffset.current.x),
            y: snap(world.y - dragOffset.current.y),
          },
        });
        return;
      }
      if (!panning) return;
      const dx = event.clientX - panStart.current.x;
      const dy = event.clientY - panStart.current.y;
      setView((v) => ({
        ...v,
        panX: panStart.current.panX + dx,
        panY: panStart.current.panY + dy,
      }));
    },
    [
      draggingDevice,
      draggingRemote,
      sessionLayout,
      onSessionLayoutChange,
      layout,
      onLayoutChange,
      panning,
      screenToWorld,
    ],
  );

  const onViewportPointerUp = () => {
    if (draggingRemote && layout && onLayoutChange) {
      onLayoutChange(withInferredEdges(layout));
    }
    setDraggingRemote(false);
    setDraggingDevice(null);
    setPanning(false);
  };

  const onWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    const rect = viewportRef.current?.getBoundingClientRect();
    if (!rect) return;
    const mx = event.clientX - rect.left;
    const my = event.clientY - rect.top;
    const factor = event.deltaY > 0 ? 0.92 : 1.08;
    setView((v) => {
      const newZoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v.zoom * factor));
      const worldX = (mx - v.panX) / v.zoom;
      const worldY = (my - v.panY) / v.zoom;
      return {
        zoom: newZoom,
        panX: mx - worldX * newZoom,
        panY: my - worldY * newZoom,
      };
    });
  };

  const zoomStep = (factor: number) => {
    if (!viewportRef.current) return;
    const vw = viewportRef.current.clientWidth;
    const vh = viewportRef.current.clientHeight;
    setView((v) => {
      const newZoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v.zoom * factor));
      const cx = (vw / 2 - v.panX) / v.zoom;
      const cy = (vh / 2 - v.panY) / v.zoom;
      return {
        zoom: newZoom,
        panX: vw / 2 - cx * newZoom,
        panY: vh / 2 - cy * newZoom,
      };
    });
  };

  const exitId = runtimeForBadge?.edge.exit_local_monitor_id;
  const edgeLabel = runtimeForBadge
    ? `${runtimeForBadge.edge.local_exit} → ${runtimeForBadge.edge.remote_entry}`
    : '';
  const zoomPct = Math.round(view.zoom * 100);

  const remotePending = isSessionMode
    ? !(sessionLayout?.per_device[remoteDeviceId ?? '']?.length)
    : !layout?.remote_monitors?.length;

  const sessionEntries = isSessionMode && sessionLayout
    ? sessionAllMonitors(sessionLayout)
    : [];

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
        {!loading && !hasContent && (
          <Text size="sm" c="dimmed">{t('layoutEditor.noRemoteMember')}</Text>
        )}
        {!loading && hasContent && (
          <>
            <Group justify="space-between" align="center">
              <Group gap="md">
                <Text className={classes.edgeBadge}>{edgeLabel}</Text>
                {exitId != null && (
                  <Text size="xs" c="dimmed">
                    {t('layoutEditor.exitMonitor', { id: exitId })}
                  </Text>
                )}
              </Group>
              <Group gap={4}>
                <Tooltip label={t('layoutEditor.zoomOut')}>
                  <ActionIcon variant="subtle" color="gray" onClick={() => zoomStep(0.85)}>
                    −
                  </ActionIcon>
                </Tooltip>
                <Text size="xs" c="dimmed" w={44} ta="center">{zoomPct}%</Text>
                <Tooltip label={t('layoutEditor.zoomIn')}>
                  <ActionIcon variant="subtle" color="gray" onClick={() => zoomStep(1.15)}>
                    +
                  </ActionIcon>
                </Tooltip>
                <Button size="xs" variant="subtle" color="gray" onClick={fitToContent}>
                  {t('layoutEditor.fitView')}
                </Button>
              </Group>
            </Group>

            <Text size="xs" c="dimmed">{t('layoutEditor.panZoomHint')}</Text>

            {remotePending && (
              <Text size="sm" c="yellow">{t('layoutEditor.syncPending')}</Text>
            )}

            <Box
              ref={viewportRef}
              className={`${classes.viewport} ${panning ? classes.viewportPanning : ''}`}
              onPointerDown={onViewportPointerDown}
              onPointerMove={onViewportPointerMove}
              onPointerUp={onViewportPointerUp}
              onPointerLeave={onViewportPointerUp}
              onWheel={onWheel}
            >
              <Box
                className={classes.worldLayer}
                style={{
                  transform: `translate(${view.panX}px, ${view.panY}px) scale(${view.zoom})`,
                }}
              >
                <Box
                  className={classes.monitorCanvas}
                  style={{ width: canvasW, height: canvasH }}
                >
                  {isSessionMode && sessionEntries.map(({ deviceId, monitor }, idx) => {
                    const box = toScreen(monitor, worldShift);
                    const isLocal = deviceId === localDeviceId;
                    const isExit = isLocal && monitor.id === exitId;
                    const isDragging = draggingDevice === deviceId;
                    const monitorClass = isLocal ? classes.localMonitor : classes.remoteMonitor;
                    const label = isLocal
                      ? `${t('layoutEditor.localLabel')} ${idx + 1}`
                      : `${remoteLabel ?? t('layoutEditor.remoteLabel')} ${idx + 1}`;
                    return (
                      <Box
                        key={`${deviceId}-${monitor.id}`}
                        className={`${monitorClass} ${isDragging ? classes.remoteMonitorDragging : ''} ${isExit ? classes.localMonitorExit : ''}`}
                        style={{
                          left: box.left,
                          top: box.top,
                          width: box.width,
                          height: box.height,
                        }}
                        onPointerDown={(e) => onDevicePointerDown(deviceId, e)}
                      >
                        <span>{label}</span>
                        <span className={classes.resBadge}>{formatResolution(monitor)}</span>
                        {isExit && (
                          <span className={classes.resBadge}>{t('layoutEditor.crossingEdge')}</span>
                        )}
                      </Box>
                    );
                  })}

                  {!isSessionMode && layout && layout.local_monitors.map((monitor, idx) => {
                    const box = toScreen(monitor, worldShift);
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

                  {!isSessionMode && placedRemote.map((monitor, idx) => {
                    const box = toScreen(monitor, worldShift);
                    return (
                      <Box
                        key={`remote-${monitor.id}-${idx}`}
                        className={`${classes.remoteMonitor} ${draggingRemote ? classes.remoteMonitorDragging : ''}`}
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
            </Box>

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
          <Button onClick={handleSave} loading={saving} color="teal" disabled={!hasContent}>
            {t('layoutEditor.saveButton')}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
