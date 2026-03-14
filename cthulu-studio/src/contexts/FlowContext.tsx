import { createContext, useContext, useState, useEffect, useRef, useMemo, useCallback, type ReactNode } from "react";
import * as api from "../api/client";
import { log } from "../api/logger";
import { useFlowDispatch, type UpdateSource, type UpdateSignal } from "../hooks/useFlowDispatch";
import type { Flow, FlowSummary, NodeTypeSchema } from "../types/flow";

interface FlowContextValue {
  flows: FlowSummary[];
  setFlows: React.Dispatch<React.SetStateAction<FlowSummary[]>>;
  activeFlowId: string | null;
  setActiveFlowId: React.Dispatch<React.SetStateAction<string | null>>;
  nodeTypes: NodeTypeSchema[];
  setNodeTypes: React.Dispatch<React.SetStateAction<NodeTypeSchema[]>>;
  canonicalFlow: Flow | null;
  updateSignal: UpdateSignal;
  flowVersionRef: React.RefObject<number>;
  dispatchFlowUpdate: (source: UpdateSource, updates: Partial<Flow>) => void;
  initFlow: (flow: Flow) => void;
  activeFlowMeta: { id: string; name: string; description: string; enabled: boolean } | null;
  loadFlows: () => Promise<void>;
  loadNodeTypes: () => Promise<void>;
}

const FlowContext = createContext<FlowContextValue | null>(null);

export function FlowProvider({ children }: { children: ReactNode }) {
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [nodeTypes, setNodeTypes] = useState<NodeTypeSchema[]>([]);

  const activeFlowIdRef = useRef(activeFlowId);
  activeFlowIdRef.current = activeFlowId;

  const loadFlows = useCallback(async () => {
    try { setFlows(await api.listFlows()); } catch { /* logged */ }
  }, []);

  const loadNodeTypes = useCallback(async () => {
    try { setNodeTypes(await api.getNodeTypes()); } catch { /* logged */ }
  }, []);

  const dispatchApi = useMemo(() => ({
    onSaveComplete: loadFlows,
    updateFlow: api.updateFlow,
    getFlow: api.getFlow,
  }), [loadFlows]);

  const {
    canonicalFlow,
    updateSignal,
    flowVersionRef,
    dispatchFlowUpdate,
    initFlow,
  } = useFlowDispatch(dispatchApi, activeFlowIdRef);

  const activeFlowMeta = useMemo(() => {
    if (!canonicalFlow || !activeFlowId) return null;
    return { id: canonicalFlow.id, name: canonicalFlow.name, description: canonicalFlow.description, enabled: canonicalFlow.enabled };
  }, [canonicalFlow, activeFlowId]);

  // Boot: load flows and node types on mount
  const initialized = useRef(false);
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    log("info", "Cthulu Studio started (Tauri IPC mode)");
    loadFlows();
    loadNodeTypes();
  }, [loadFlows, loadNodeTypes]);

  return (
    <FlowContext.Provider
      value={{
        flows,
        setFlows,
        activeFlowId,
        setActiveFlowId,
        nodeTypes,
        setNodeTypes,
        canonicalFlow,
        updateSignal,
        flowVersionRef,
        dispatchFlowUpdate,
        initFlow,
        activeFlowMeta,
        loadFlows,
        loadNodeTypes,
      }}
    >
      {children}
    </FlowContext.Provider>
  );
}

export function useFlowContext(): FlowContextValue {
  const ctx = useContext(FlowContext);
  if (!ctx) throw new Error("useFlowContext must be used within FlowProvider");
  return ctx;
}
