import { create } from "zustand";
import type { ActiveView } from "../types/flow";

interface ViewStore {
  activeView: ActiveView;
  selectedNodeId: string | null;
  selectedPromptId: string | null;
  promptListKey: number;
  sidebarCollapsed: boolean;
  showSettings: boolean;

  setActiveView: (view: ActiveView) => void;
  setSelectedNodeId: (id: string | null) => void;
  setSelectedPromptId: (id: string | null) => void;
  bumpPromptListKey: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setShowSettings: (show: boolean) => void;
}

export const useViewStore = create<ViewStore>((set) => ({
  activeView: "flow-editor",
  selectedNodeId: null,
  selectedPromptId: null,
  promptListKey: 0,
  sidebarCollapsed: false,
  showSettings: false,

  setActiveView: (view) => set({ activeView: view }),
  setSelectedNodeId: (id) => set({ selectedNodeId: id }),
  setSelectedPromptId: (id) => set({ selectedPromptId: id }),
  bumpPromptListKey: () =>
    set((state) => ({ promptListKey: state.promptListKey + 1 })),
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),
  setShowSettings: (show) => set({ showSettings: show }),
}));
