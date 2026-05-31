import type { MonitorLayoutDto, MonitorRectDto } from '../ipc/commands';

export const REMOTE_MONITOR_ID = 65535;

interface Bounds {
  minX: number;
  minY: number;
  maxR: number;
  maxB: number;
}

export function localUnionBounds(monitors: MonitorRectDto[]): Bounds {
  const first = monitors[0];
  if (!first) {
    return { minX: 0, minY: 0, maxR: 1920, maxB: 1080 };
  }
  let minX = first.x;
  let minY = first.y;
  let maxR = first.x + first.width;
  let maxB = first.y + first.height;
  for (const m of monitors.slice(1)) {
    minX = Math.min(minX, m.x);
    minY = Math.min(minY, m.y);
    maxR = Math.max(maxR, m.x + m.width);
    maxB = Math.max(maxB, m.y + m.height);
  }
  return { minX, minY, maxR, maxB };
}

export function inferEdgesFromGeometry(layout: MonitorLayoutDto): MonitorLayoutDto {
  const bounds = localUnionBounds(layout.local_monitors);
  const localCx = (bounds.minX + bounds.maxR) / 2;
  const localCy = (bounds.minY + bounds.maxB) / 2;
  const rv = layout.remote_virtual;
  const remoteCx = rv.x + rv.width / 2;
  const remoteCy = rv.y + rv.height / 2;
  const dx = remoteCx - localCx;
  const dy = remoteCy - localCy;

  let local_exit: string;
  let remote_entry: string;

  if (Math.abs(dx) >= Math.abs(dy)) {
    if (dx >= 0) {
      local_exit = 'Right';
      remote_entry = 'Left';
    } else {
      local_exit = 'Left';
      remote_entry = 'Right';
    }
  } else if (dy >= 0) {
    local_exit = 'Bottom';
    remote_entry = 'Top';
  } else {
    local_exit = 'Top';
    remote_entry = 'Bottom';
  }

  return {
    ...layout,
    edge: { local_exit, remote_entry },
  };
}

export function buildDefaultLayout(
  localMonitors: MonitorRectDto[],
  remotePeerId: string,
): MonitorLayoutDto {
  const monitors =
    localMonitors.length > 0
      ? localMonitors
      : [{ id: 1, x: 0, y: 0, width: 1920, height: 1080 }];
  const bounds = localUnionBounds(monitors);
  const width = Math.max(...monitors.map((m) => m.width));
  const height = Math.max(...monitors.map((m) => m.height));

  return inferEdgesFromGeometry({
    local_monitors: monitors,
    remote_peer: remotePeerId,
    remote_virtual: {
      id: REMOTE_MONITOR_ID,
      x: bounds.maxR,
      y: bounds.minY,
      width,
      height,
    },
    edge: { local_exit: 'Right', remote_entry: 'Left' },
  });
}

export function withInferredEdges(layout: MonitorLayoutDto): MonitorLayoutDto {
  return inferEdgesFromGeometry(layout);
}
