import { createContext, useContext, useState, type ReactNode } from "react";
import type { ActiveView } from "../types/flow";

interface NavigationContextValue {
  activeView: ActiveView;
  setActiveView: React.Dispatch<React.SetStateAction<ActiveView>>;
  selectedNodeId: string | null;
  setSelectedNodeId: React.Dispatch<React.SetStateAction<string | null>>;
  selectedAgentId: string | null;
  setSelectedAgentId: React.Dispatch<React.SetStateAction<string | null>>;
  selectedSessionId: string | null;
  setSelectedSessionId: React.Dispatch<React.SetStateAction<string | null>>;
  selectedAgentName: string | null;
  setSelectedAgentName: React.Dispatch<React.SetStateAction<string | null>>;
  visitedAgents: Map<string, { name: string; sessionId: string }>;
  setVisitedAgents: React.Dispatch<React.SetStateAction<Map<string, { name: string; sessionId: string }>>>;
  selectedPromptId: string | null;
  setSelectedPromptId: React.Dispatch<React.SetStateAction<string | null>>;
  editingWorkflow: { workspace: string; name: string } | null;
  setEditingWorkflow: React.Dispatch<React.SetStateAction<{ workspace: string; name: string } | null>>;
}

const NavigationContext = createContext<NavigationContextValue | null>(null);

export function NavigationProvider({ children }: { children: ReactNode }) {
  const [activeView, setActiveView] = useState<ActiveView>("agent-list");
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedAgentName, setSelectedAgentName] = useState<string | null>(null);
  const [visitedAgents, setVisitedAgents] = useState<Map<string, { name: string; sessionId: string }>>(new Map());
  const [selectedPromptId, setSelectedPromptId] = useState<string | null>(null);
  const [editingWorkflow, setEditingWorkflow] = useState<{ workspace: string; name: string } | null>(null);

  return (
    <NavigationContext.Provider
      value={{
        activeView,
        setActiveView,
        selectedNodeId,
        setSelectedNodeId,
        selectedAgentId,
        setSelectedAgentId,
        selectedSessionId,
        setSelectedSessionId,
        selectedAgentName,
        setSelectedAgentName,
        visitedAgents,
        setVisitedAgents,
        selectedPromptId,
        setSelectedPromptId,
        editingWorkflow,
        setEditingWorkflow,
      }}
    >
      {children}
    </NavigationContext.Provider>
  );
}

export function useNavigation(): NavigationContextValue {
  const ctx = useContext(NavigationContext);
  if (!ctx) throw new Error("useNavigation must be used within NavigationProvider");
  return ctx;
}
