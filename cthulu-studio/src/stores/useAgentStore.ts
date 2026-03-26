import { create } from "zustand";
import * as api from "../api/client";

interface VisitedAgent {
  name: string;
  sessionId: string;
}

interface AgentStore {
  selectedAgentId: string | null;
  selectedSessionId: string | null;
  selectedAgentName: string | null;
  visitedAgents: Map<string, VisitedAgent>;
  agentListKey: number;

  selectSession: (agentId: string, sessionId: string) => Promise<void>;
  clearSelection: () => void;
  removeVisited: (agentId: string) => void;
  bumpAgentListKey: () => void;
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  selectedAgentId: null,
  selectedSessionId: null,
  selectedAgentName: null,
  visitedAgents: new Map(),
  agentListKey: 0,

  selectSession: async (agentId, sessionId) => {
    try {
      const agent = await api.getAgent(agentId);
      set((state) => ({
        selectedAgentId: agentId,
        selectedSessionId: sessionId,
        selectedAgentName: agent.name,
        visitedAgents: new Map(state.visitedAgents).set(agentId, {
          name: agent.name,
          sessionId,
        }),
      }));
    } catch {
      // logged by api client
    }
  },

  clearSelection: () =>
    set({
      selectedAgentId: null,
      selectedSessionId: null,
      selectedAgentName: null,
    }),

  removeVisited: (agentId) =>
    set((state) => {
      const next = new Map(state.visitedAgents);
      next.delete(agentId);
      return {
        visitedAgents: next,
        selectedAgentId: null,
        selectedSessionId: null,
        selectedAgentName: null,
      };
    }),

  bumpAgentListKey: () =>
    set((state) => ({ agentListKey: state.agentListKey + 1 })),
}));
