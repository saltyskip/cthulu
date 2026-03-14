import { createContext, useContext, useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import { subscribeToRuns } from "../api/runStream";
import type { RunEvent } from "../types/flow";

interface RunContextValue {
  runEvents: RunEvent[];
  setRunEvents: React.Dispatch<React.SetStateAction<RunEvent[]>>;
  nodeRunStatus: Record<string, "running" | "completed" | "failed">;
  setNodeRunStatus: React.Dispatch<React.SetStateAction<Record<string, "running" | "completed" | "failed">>>;
  runLogOpen: boolean;
  setRunLogOpen: React.Dispatch<React.SetStateAction<boolean>>;
  handleRunEvent: (event: RunEvent) => void;
}

const RunContext = createContext<RunContextValue | null>(null);

export function RunProvider({ activeFlowId, children }: { activeFlowId: string | null; children: ReactNode }) {
  const [runEvents, setRunEvents] = useState<RunEvent[]>([]);
  const [nodeRunStatus, setNodeRunStatus] = useState<Record<string, "running" | "completed" | "failed">>({});
  const [runLogOpen, setRunLogOpen] = useState(false);

  const clearTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleRunEvent = useCallback((event: RunEvent) => {
    setRunEvents((prev) => {
      const next = [...prev, event];
      return next.length > 500 ? next.slice(-500) : next;
    });

    // Auto-open run log when a run starts
    if (event.event_type === "run_started") {
      if (clearTimer.current) clearTimeout(clearTimer.current);
      setNodeRunStatus({});
      setRunLogOpen(true);
    }

    if (event.node_id) {
      if (event.event_type === "node_started") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "running" }));
      } else if (event.event_type === "node_completed") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "completed" }));
      } else if (event.event_type === "node_failed") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "failed" }));
      }
    }

    if (event.event_type === "run_completed" || event.event_type === "run_failed") {
      clearTimer.current = setTimeout(() => setNodeRunStatus({}), 10000);
    }
  }, []);

  // SSE subscription for run events
  useEffect(() => {
    if (!activeFlowId) return;
    setRunEvents([]);
    setNodeRunStatus({});
    const cleanup = subscribeToRuns(activeFlowId, handleRunEvent);
    return cleanup;
  }, [activeFlowId, handleRunEvent]);

  return (
    <RunContext.Provider
      value={{
        runEvents,
        setRunEvents,
        nodeRunStatus,
        setNodeRunStatus,
        runLogOpen,
        setRunLogOpen,
        handleRunEvent,
      }}
    >
      {children}
    </RunContext.Provider>
  );
}

export function useRunContext(): RunContextValue {
  const ctx = useContext(RunContext);
  if (!ctx) throw new Error("useRunContext must be used within RunProvider");
  return ctx;
}
