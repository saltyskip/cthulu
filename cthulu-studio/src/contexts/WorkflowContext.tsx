import { createContext, useContext, useState, useCallback, type ReactNode } from "react";
import type { WorkflowSummary } from "../types/flow";

interface WorkflowContextValue {
  wfWorkspaces: string[];
  setWfWorkspaces: React.Dispatch<React.SetStateAction<string[]>>;
  wfActiveWorkspace: string | null;
  setWfActiveWorkspace: React.Dispatch<React.SetStateAction<string | null>>;
  wfWorkflows: WorkflowSummary[];
  setWfWorkflows: React.Dispatch<React.SetStateAction<WorkflowSummary[]>>;
  showNewWorkspace: boolean;
  setShowNewWorkspace: React.Dispatch<React.SetStateAction<boolean>>;
  newWorkflowTrigger: number;
  setNewWorkflowTrigger: React.Dispatch<React.SetStateAction<number>>;
  enabledWorkflows: Set<string>;
  toggleWorkflowEnabled: (workspace: string, name: string) => void;
  isWorkflowEnabled: (workspace: string, name: string) => boolean;
  workflowSearch: string;
  setWorkflowSearch: React.Dispatch<React.SetStateAction<string>>;
}

const WorkflowContext = createContext<WorkflowContextValue | null>(null);

export function WorkflowProvider({ children }: { children: ReactNode }) {
  const [wfWorkspaces, setWfWorkspaces] = useState<string[]>([]);
  const [wfActiveWorkspace, setWfActiveWorkspaceRaw] = useState<string | null>(null);
  const [wfWorkflows, setWfWorkflows] = useState<WorkflowSummary[]>([]);
  const [showNewWorkspace, setShowNewWorkspace] = useState(false);
  const [newWorkflowTrigger, setNewWorkflowTrigger] = useState(0);
  const [enabledWorkflows, setEnabledWorkflows] = useState<Set<string>>(new Set());
  const [workflowSearch, setWorkflowSearch] = useState("");

  // Clear search when workspace changes
  const setWfActiveWorkspace: React.Dispatch<React.SetStateAction<string | null>> = useCallback((action) => {
    setWfActiveWorkspaceRaw((prev) => {
      const next = typeof action === "function" ? action(prev) : action;
      if (next !== prev) setWorkflowSearch("");
      return next;
    });
  }, []);

  const getWorkflowKey = (workspace: string, name: string) => `${workspace}::${name}`;

  const toggleWorkflowEnabled = useCallback((workspace: string, name: string) => {
    const key = getWorkflowKey(workspace, name);
    setEnabledWorkflows((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const isWorkflowEnabled = useCallback((workspace: string, name: string) => {
    return enabledWorkflows.has(getWorkflowKey(workspace, name));
  }, [enabledWorkflows]);

  return (
    <WorkflowContext.Provider
      value={{
        wfWorkspaces,
        setWfWorkspaces,
        wfActiveWorkspace,
        setWfActiveWorkspace,
        wfWorkflows,
        setWfWorkflows,
        showNewWorkspace,
        setShowNewWorkspace,
        newWorkflowTrigger,
        setNewWorkflowTrigger,
        enabledWorkflows,
        toggleWorkflowEnabled,
        isWorkflowEnabled,
        workflowSearch,
        setWorkflowSearch,
      }}
    >
      {children}
    </WorkflowContext.Provider>
  );
}

export function useWorkflowContext(): WorkflowContextValue {
  const ctx = useContext(WorkflowContext);
  if (!ctx) throw new Error("useWorkflowContext must be used within WorkflowProvider");
  return ctx;
}
