import type { MonitorLayoutDto, MonitorRectDto } from '../ipc/commands';

export const REMOTE_MONITOR_ID = 65535;
const ADJACENCY_GAP_PX = 80;

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

function overlap1d(a0: number, a1: number, b0: number, b1: number): number {
  return Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
}

export function placedRemoteMonitors(layout: MonitorLayoutDto): MonitorRectDto[] {
  if (!layout.remote_monitors?.length) {
    return [layout.remote_virtual];
  }
  const minX = Math.min(...layout.remote_monitors.map((m) => m.x));
  const minY = Math.min(...layout.remote_monitors.map((m) => m.y));
  const ox = layout.remote_virtual.x;
  const oy = layout.remote_virtual.y;
  return layout.remote_monitors.map((m) => ({
    ...m,
    x: m.x - minX + ox,
    y: m.y - minY + oy,
  }));
}

export function placedRemoteBounds(layout: MonitorLayoutDto): MonitorRectDto {
  const placed = placedRemoteMonitors(layout);
  const first = placed[0];
  if (!first) return layout.remote_virtual;
  let minX = first.x;
  let minY = first.y;
  let maxR = first.x + first.width;
  let maxB = first.y + first.height;
  for (const m of placed.slice(1)) {
    minX = Math.min(minX, m.x);
    minY = Math.min(minY, m.y);
    maxR = Math.max(maxR, m.x + m.width);
    maxB = Math.max(maxB, m.y + m.height);
  }
  return {
    id: REMOTE_MONITOR_ID,
    x: minX,
    y: minY,
    width: maxR - minX,
    height: maxB - minY,
  };
}

function findAdjacentEdge(
  locals: MonitorRectDto[],
  remote: MonitorRectDto,
): { exitId: number; local_exit: string; remote_entry: string } {
  type Cand = { exitId: number; local_exit: string; remote_entry: string; score: number };
  let best: Cand | null = null;

  const consider = (
    exitId: number,
    local_exit: string,
    remote_entry: string,
    gap: number,
    overlap: number,
  ) => {
    if (overlap <= 0 || Math.abs(gap) > ADJACENCY_GAP_PX) return;
    const score = Math.abs(gap) * 10_000 - overlap;
    if (!best || score < best.score) {
      best = { exitId, local_exit, remote_entry, score };
    }
  };

  for (const local of locals) {
    const localB = local.y + local.height;
    const localR = local.x + local.width;
    const remoteB = remote.y + remote.height;
    const remoteR = remote.x + remote.width;
    const overlapY = overlap1d(local.y, localB, remote.y, remoteB);
    consider(local.id, 'Right', 'Left', remote.x - localR, overlapY);
    consider(local.id, 'Left', 'Right', local.x - remoteR, overlapY);
    const overlapX = overlap1d(local.x, localR, remote.x, remoteR);
    consider(local.id, 'Bottom', 'Top', remote.y - localB, overlapX);
    consider(local.id, 'Top', 'Bottom', local.y - remoteB, overlapX);
  }

  const fallbackId = locals[0]?.id ?? 1;
  return best ?? { exitId: fallbackId, local_exit: 'Right', remote_entry: 'Left' };
}

export function inferEdgesFromGeometry(layout: MonitorLayoutDto): MonitorLayoutDto {
  const remote = placedRemoteBounds(layout);
  const { exitId, local_exit, remote_entry } = findAdjacentEdge(
    layout.local_monitors,
    remote,
  );
  return {
    ...layout,
    edge: {
      local_exit,
      remote_entry,
      exit_local_monitor_id: exitId,
    },
  };
}

export function buildDefaultLayout(
  localMonitors: MonitorRectDto[],
  remotePeerId: string,
  remoteMonitors: MonitorRectDto[] = [],
): MonitorLayoutDto {
  const monitors =
    localMonitors.length > 0
      ? localMonitors
      : [{ id: 1, x: 0, y: 0, width: 1920, height: 1080 }];
  const localBounds = localUnionBounds(monitors);

  let remoteVirtual: MonitorRectDto;
  let remoteList = remoteMonitors;

  if (remoteMonitors.length > 0) {
    const rb = localUnionBounds(remoteMonitors);
    const rw = rb.maxR - rb.minX;
    const rh = rb.maxB - rb.minY;
    remoteVirtual = {
      id: REMOTE_MONITOR_ID,
      x: localBounds.maxR,
      y: localBounds.minY,
      width: rw,
      height: rh,
    };
  } else {
    const width = Math.max(...monitors.map((m) => m.width));
    const height = Math.max(...monitors.map((m) => m.height));
    remoteVirtual = {
      id: REMOTE_MONITOR_ID,
      x: localBounds.maxR,
      y: localBounds.minY,
      width,
      height,
    };
    remoteList = [];
  }

  return inferEdgesFromGeometry({
    local_monitors: monitors,
    remote_peer: remotePeerId,
    remote_virtual: remoteVirtual,
    remote_monitors: remoteList,
    edge: { local_exit: 'Right', remote_entry: 'Left', exit_local_monitor_id: monitors[0]?.id ?? 1 },
  });
}

export function withInferredEdges(layout: MonitorLayoutDto): MonitorLayoutDto {
  return inferEdgesFromGeometry(layout);
}

/** Escala o canvas para caber no viewport mantendo proporção pixel-perfect. */
export function computeCanvasScale(
  layout: MonitorLayoutDto,
  maxWidth: number,
  maxHeight: number,
): number {
  const localBounds = localUnionBounds(layout.local_monitors);
  const remoteBounds = placedRemoteBounds(layout);
  const totalW = Math.max(localBounds.maxR, remoteBounds.x + remoteBounds.width) + 80;
  const totalH = Math.max(localBounds.maxB, remoteBounds.y + remoteBounds.height) + 80;
  const scaleW = maxWidth / totalW;
  const scaleH = maxHeight / totalH;
  return Math.min(scaleW, scaleH, 0.2);
}

export function formatResolution(m: MonitorRectDto): string {
  return `${m.width}×${m.height}`;
}
