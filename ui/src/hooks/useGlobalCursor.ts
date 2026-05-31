import { useEffect, useState } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { onWinxEvent } from '../ipc/events';

export interface GlobalCursorPosition {
  x: number;
  y: number;
  seq: number;
}

export function useGlobalCursor(activeWorkspaceId: string | null): GlobalCursorPosition | null {
  const [position, setPosition] = useState<GlobalCursorPosition | null>(null);

  useEffect(() => {
    if (!activeWorkspaceId) {
      setPosition(null);
      return;
    }

    let unlisten: UnlistenFn | undefined;

    void onWinxEvent((event) => {
      if (
        event.kind === 'workspace-global-cursor' &&
        event.workspace_id === activeWorkspaceId
      ) {
        setPosition({
          x: event.x,
          y: event.y,
          seq: event.seq ?? 0,
        });
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [activeWorkspaceId]);

  return position;
}
