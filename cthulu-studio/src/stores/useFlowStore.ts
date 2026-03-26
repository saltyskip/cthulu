import { create } from "zustand";
import * as api from "../api/client";
import type { FlowSummary, NodeTypeSchema, RunEvent } from "../types/flow";

interface FlowStore {
  // Flow list
  flows: FlowSummary[];
  activeFlowId: string | null;
  nodeTypes: NodeTypeSchema[];

  // Run state
  runEvents: RunEvent[];
  nodeRunStatus: Record<string, "running" | "completed" | "failed">;
  runLogOpen: boolean;

  // Actions
  loadFlows: () => Promise<void>;
  loadNodeTypes: () => Promise<void>;
  setActiveFlowId: (id: string | null) => void;
  setRunLogOpen: (open: boolean) => void;
  clearRunEvents: () => void;
  addRunEvent: (event: RunEvent) => void;
  setNodeRunStatus: (status: Record<string, "running" | "completed" | "failed">) => void;
  toggleFlowEnabled: (flowId: string) => Promise<void>;
}

export const useFlowStore = create<FlowStore>((set, get) => ({
  flows: [],
  activeFlowId: null,
  nodeTypes: [],
  runEvents: [],
  nodeRunStatus: {},
  runLogOpen: false,

  loadFlows: async () => {
    try {
      const flows = await api.listFlows();
      set({ flows });
    } catch {
      // logged by api client
    }
  },

  loadNodeTypes: async () => {
    try {
      const nodeTypes = await api.getNodeTypes();
      set({ nodeTypes });
    } catch {
      // logged by api client
    }
  },

  setActiveFlowId: (id) => set({ activeFlowId: id }),
  setRunLogOpen: (open) => set({ runLogOpen: open }),
  clearRunEvents: () => set({ runEvents: [], nodeRunStatus: {} }),

  addRunEvent: (event) => {
    set((state) => {
      const runEvents = [...state.runEvents, event];
      const trimmed = runEvents.length > 500 ? runEvents.slice(-500) : runEvents;

      let nodeRunStatus = state.nodeRunStatus;
      if (event.event_type === "run_started") {
        nodeRunStatus = {};
      }
      if (event.node_id) {
        if (event.event_type === "node_started") {
          nodeRunStatus = { ...nodeRunStatus, [event.node_id]: "running" };
        } else if (event.event_type === "node_completed") {
          nodeRunStatus = { ...nodeRunStatus, [event.node_id]: "completed" };
        } else if (event.event_type === "node_failed") {
          nodeRunStatus = { ...nodeRunStatus, [event.node_id]: "failed" };
        }
      }

      return {
        runEvents: trimmed,
        nodeRunStatus,
        runLogOpen: event.event_type === "run_started" ? true : state.runLogOpen,
      };
    });
  },

  setNodeRunStatus: (status) => set({ nodeRunStatus: status }),

  toggleFlowEnabled: async (flowId) => {
    const { flows } = get();
    const flow = flows.find((f) => f.id === flowId);
    if (!flow) return;
    const newEnabled = !flow.enabled;

    // Optimistic update
    set({
      flows: flows.map((f) =>
        f.id === flowId ? { ...f, enabled: newEnabled } : f
      ),
    });

    try {
      await api.updateFlow(flowId, { enabled: newEnabled });
      get().loadFlows();
    } catch {
      // logged by api client
    }
  },
}));
