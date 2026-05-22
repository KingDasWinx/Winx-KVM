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

  // Actions
  refresh: () => Promise<void>;
  setPendingInvite: (invite: ipc.PendingInviteDto | null) => void;
  setConflict: (conflict: WorkspaceStoreState['conflict']) => void;
  setActiveWorkspaceId: (id: string | null) => void;
}

export const useWorkspaceStore = create<WorkspaceStoreState>((set) => ({
  workspaces: [],
  pendingInvite: null,
  conflict: null,
  activeWorkspaceId: null,

  refresh: async () => {
    try {
      const workspaces = await ipc.listWorkspaces();
      set({ workspaces });
    } catch (err) {
      console.error('Failed to refresh workspaces:', err);
    }
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
}));
