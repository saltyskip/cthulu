import { useEffect, useState, useCallback } from "react";
import type { HeartbeatRun, HeartbeatRunStatus } from "../types/flow";
import { listHeartbeatRuns, getHeartbeatRunLog } from "../api/client";

interface HeartbeatRunsPanelProps {
  agentId: string;
}

const STATUS_COLORS: Record<HeartbeatRunStatus, string> = {
  queued: "var(--text-secondary)",
  running: "var(--accent)",
  succeeded: "#22c55e",
  failed: "#ef4444",
  timed_out: "#f59e0b",
  cancelled: "var(--text-secondary)",
};

export default function HeartbeatRunsPanel({ agentId }: HeartbeatRunsPanelProps) {
  const [runs, setRuns] = useState<HeartbeatRun[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const data = await listHeartbeatRuns(agentId);
      setRuns(data);
    } catch {
      /* ignore */
    }
    setLoading(false);
  }, [agentId]);

  useEffect(() => {
    refresh();
    const interval = setInterval(() => {
      if (runs.some((r) => r.status === "running" || r.status === "queued")) {
        refresh();
      }
    }, 5000);
    return () => clearInterval(interval);
  }, [agentId, refresh, runs]);

  const viewLog = async (runId: string) => {
    setSelectedRunId(runId);
    try {
      const data = await getHeartbeatRunLog(agentId, runId);
      setLogLines(data.lines);
    } catch {
      setLogLines(["Failed to load log."]);
    }
  };

  const formatDuration = (secs: number) => {
    if (secs < 60) return `${secs.toFixed(1)}s`;
    const mins = Math.floor(secs / 60);
    const remainSecs = (secs % 60).toFixed(0);
    return `${mins}m ${remainSecs}s`;
  };

  const formatTime = (iso: string) => {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  if (loading) return <div className="hbr-loading">Loading runs...</div>;
  if (runs.length === 0)
    return (
      <div className="hbr-empty">
        No heartbeat runs yet. Enable heartbeat in the agent editor or trigger a manual run.
      </div>
    );

  return (
    <div className="hbr-panel">
      <div className="hbr-list">
        {runs.map((run) => (
          <div
            key={run.id}
            className={`hbr-row ${selectedRunId === run.id ? "hbr-row-selected" : ""}`}
            onClick={() => viewLog(run.id)}
          >
            <span
              className="hbr-status-dot"
              style={{ background: STATUS_COLORS[run.status] }}
            />
            <span className="hbr-status">{run.status.replace("_", " ")}</span>
            <span className="hbr-time">{formatTime(run.started_at)}</span>
            <span className="hbr-duration">{formatDuration(run.duration_secs)}</span>
            <span className="hbr-cost">${run.cost_usd.toFixed(4)}</span>
            {run.model && <span className="hbr-model">{run.model}</span>}
            {run.error && (
              <span className="hbr-error" title={run.error}>
                !
              </span>
            )}
          </div>
        ))}
      </div>
      {selectedRunId && (
        <div className="hbr-log">
          <div className="hbr-log-header">
            Run {selectedRunId.slice(0, 8)}
            {runs.find((r) => r.id === selectedRunId)?.error && (
              <span className="hbr-log-error">
                {runs.find((r) => r.id === selectedRunId)?.error}
              </span>
            )}
          </div>
          <pre className="hbr-log-content">
            {logLines.length > 0 ? logLines.join("\n") : "No log output."}
          </pre>
        </div>
      )}
    </div>
  );
}
