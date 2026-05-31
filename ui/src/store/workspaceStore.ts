import { create } from 'zustand';
import * as ipc from '../ipc/commands';

export interface WorkspaceStoreState {
  workspaces: ipc.WorkspaceDto[];
  pendingInvite: ipc.PendingInviteDto | null;
  conflict: {
    activeId: string;
    targetId: string;
    activeName: string;
    targetName: string;
  } | null;
  activeWorkspaceId: string | null;
  presence: Record<string, boolean>;

  refresh: () => Promise<void>;
  setPendingInvite: (invite: ipc.PendingInviteDto | null) => void;
  setConflict: (conflict: WorkspaceStoreState['conflict']) => void;
  setActiveWorkspaceId: (id: string | null) => void;
  setPresence: (workspaceId: string, deviceId: string, isOnline: boolean) => void;
  clearWorkspacePresence: (workspaceId: string) => void;
}

export const useWorkspaceStore = create<WorkspaceStoreState>((set) => ({
  workspaces: [],
  pendingInvite: null,
  conflict: null,
  activeWorkspaceId: null,
  presence: {},

  refresh: async () => {
    const workspaces = await ipc.listWorkspaces();
    set({ workspaces });
  },

  setPendingInvite: (invite) => {
    set({ pendingInvite: invite });
  },

  setConflict: (conflict) => {
    set({ conflict });
  },

  setActiveWorkspaceId: (id) => {
    set({ activeWorkspaceId: id });
  },

  setPresence: (workspaceId, deviceId, isOnline) =>
    set((state) => ({
      presence: { ...state.presence, [`${workspaceId}:${deviceId}`]: isOnline },
    })),

  clearWorkspacePresence: (workspaceId) =>
    set((state) => {
      const prefix = `${workspaceId}:`;
      const presence = Object.fromEntries(
        Object.entries(state.presence).filter(([key]) => !key.startsWith(prefix))
      );
      return { presence };
    }),
}));
