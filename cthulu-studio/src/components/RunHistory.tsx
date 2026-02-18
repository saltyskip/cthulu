import { useState, useEffect } from "react";
import * as api from "../api/client";
import type { FlowRun } from "../types/flow";

interface RunHistoryProps {
  flowId: string | null;
}

export default function RunHistory({ flowId }: RunHistoryProps) {
  const [runs, setRuns] = useState<FlowRun[]>([]);

  useEffect(() => {
    if (!flowId) {
      setRuns([]);
      return;
    }

    const fetchRuns = async () => {
      try {
        const data = await api.getFlowRuns(flowId);
        setRuns(data);
      } catch {
        // Ignore errors
      }
    };

    fetchRuns();
    const interval = setInterval(fetchRuns, 5000);
    return () => clearInterval(interval);
  }, [flowId]);

  if (!flowId || runs.length === 0) return null;

  return (
    <div className="run-history">
      <h3>Recent Runs</h3>
      {runs.slice(0, 10).map((run) => (
        <div key={run.id} className="run-item">
          <div className={`run-status ${run.status}`} />
          <span>{run.status}</span>
          <span className="run-time">
            {new Date(run.started_at).toLocaleTimeString()}
          </span>
          {run.error && (
            <span style={{ color: "var(--danger)", fontSize: 11 }}>
              {run.error.slice(0, 60)}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
