import { useState, useEffect, useMemo, useRef } from "react";
import * as api from "../api/client";
import type { HeartbeatRun } from "../types/flow";
import { StatusBadge } from "./StatusBadge";
import { runStatusDot } from "../lib/status-colors";

function formatRelativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  if (ms < 60_000) return "just now";
  const min = Math.floor(ms / 60_000);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.floor(hr / 24)}d ago`;
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

interface AgentRunsTabProps {
  agentId: string;
}

export function AgentRunsTab({ agentId }: AgentRunsTabProps) {
  const [runs, setRuns] = useState<HeartbeatRun[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [loadingLog, setLoadingLog] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  // Load runs
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      api.listHeartbeatRuns(agentId).then(list => {
        if (!cancelled) {
          setRuns(list);
          if (!selectedRunId && list.length > 0) setSelectedRunId(list[0].id);
        }
      }).catch(() => {});
    };
    load();
    const iv = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(iv); };
  }, [agentId]);

  // Load log when run is selected
  useEffect(() => {
    if (!selectedRunId) { setLogLines([]); return; }
    let cancelled = false;
    setLoadingLog(true);
    api.getHeartbeatRunLog(agentId, selectedRunId)
      .then(res => { if (!cancelled) setLogLines(res.lines); })
      .catch(() => { if (!cancelled) setLogLines(["Failed to load log"]); })
      .finally(() => { if (!cancelled) setLoadingLog(false); });
    return () => { cancelled = true; };
  }, [agentId, selectedRunId]);

  const selectedRun = useMemo(() =>
    runs.find(r => r.id === selectedRunId) ?? null, [runs, selectedRunId]);

  return (
    <div className="runs-tab">
      {/* Run List (left panel) */}
      <div className="runs-list-panel">
        <div className="runs-list-header">
          <span>Runs</span>
          <span className="runs-list-count">{runs.length}</span>
        </div>
        <div className="runs-list-scroll">
          {runs.map(run => {
            const dotColor = runStatusDot[run.status] ?? "var(--text-secondary)";
            const isSelected = run.id === selectedRunId;
            return (
              <div
                key={run.id}
                className={`runs-list-item${isSelected ? " runs-list-item-selected" : ""}`}
                onClick={() => setSelectedRunId(run.id)}
              >
                <div className="runs-list-item-dot" style={{ background: dotColor }} />
                <div className="runs-list-item-body">
                  <div className="runs-list-item-top">
                    <span className="runs-list-item-id">{run.id.slice(0, 8)}</span>
                    <span className="runs-list-item-time">{formatRelativeTime(run.started_at)}</span>
                  </div>
                  <div className="runs-list-item-bottom">
                    <StatusBadge status={run.status} />
                    <span className="runs-list-item-duration">{formatDuration(run.duration_secs)}</span>
                    <span className="runs-list-item-cost">${run.cost_usd.toFixed(4)}</span>
                  </div>
                </div>
              </div>
            );
          })}
          {runs.length === 0 && (
            <div className="runs-list-empty">No runs yet</div>
          )}
        </div>
      </div>

      {/* Run Detail (right panel) */}
      <div className="runs-detail-panel">
        {selectedRun ? (
          <>
            <div className="runs-detail-header">
              <div className="runs-detail-header-top">
                <StatusBadge status={selectedRun.status} />
                <span className="runs-detail-run-id">{selectedRun.id}</span>
              </div>
              <div className="runs-detail-meta">
                <span>Started: {new Date(selectedRun.started_at).toLocaleString()}</span>
                {selectedRun.finished_at && <span>Finished: {new Date(selectedRun.finished_at).toLocaleString()}</span>}
                <span>Duration: {formatDuration(selectedRun.duration_secs)}</span>
                <span>Cost: ${selectedRun.cost_usd.toFixed(4)}</span>
                {selectedRun.model && <span>Model: {selectedRun.model}</span>}
              </div>
              {selectedRun.error && (
                <div className="runs-detail-error">{selectedRun.error}</div>
              )}
              {selectedRun.usage && (
                <div className="runs-detail-tokens">
                  <span>Input: {selectedRun.usage.input_tokens.toLocaleString()}</span>
                  <span>Output: {selectedRun.usage.output_tokens.toLocaleString()}</span>
                  <span>Cached: {selectedRun.usage.cached_input_tokens.toLocaleString()}</span>
                </div>
              )}
            </div>
            <div className="runs-detail-log-header">
              <span>Log</span>
            </div>
            <pre className="runs-detail-log" ref={logRef}>
              {loadingLog ? "Loading..." : logLines.join("\n") || "No log output"}
            </pre>
          </>
        ) : (
          <div className="runs-detail-empty">Select a run to view details</div>
        )}
      </div>
    </div>
  );
}
