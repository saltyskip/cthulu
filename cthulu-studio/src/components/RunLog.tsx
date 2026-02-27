import { useState, useEffect, useRef } from "react";
import type { RunEvent } from "../types/flow";
import { Button } from "@/components/ui/button";

const EVENT_COLORS: Record<string, string> = {
  run_started: "var(--accent)",
  node_started: "var(--accent)",
  node_completed: "#3fb950",
  node_failed: "var(--danger)",
  run_completed: "#3fb950",
  run_failed: "var(--danger)",
  log: "var(--text-secondary)",
};

const EVENT_LABELS: Record<string, string> = {
  run_started: "START",
  node_started: "NODE",
  node_completed: "DONE",
  node_failed: "FAIL",
  run_completed: "DONE",
  run_failed: "FAIL",
  log: "LOG",
};

interface RunLogProps {
  events: RunEvent[];
  onClear: () => void;
  onClose: () => void;
}

export default function RunLog({ events, onClear, onClose }: RunLogProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [events, autoScroll]);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  };

  // Group entries by run_id for display
  const runIds = [...new Set(events.map((e) => e.run_id))];
  const latestRunId = runIds[runIds.length - 1];
  const [collapsedRuns, setCollapsedRuns] = useState<Set<string>>(new Set());

  const toggleRun = (runId: string) => {
    setCollapsedRuns((prev) => {
      const next = new Set(prev);
      if (next.has(runId)) next.delete(runId);
      else next.add(runId);
      return next;
    });
  };

  const formatTime = (ts: string) => {
    const d = new Date(ts);
    const h = String(d.getHours()).padStart(2, "0");
    const m = String(d.getMinutes()).padStart(2, "0");
    const s = String(d.getSeconds()).padStart(2, "0");
    const ms = String(d.getMilliseconds()).padStart(3, "0");
    return `${h}:${m}:${s}.${ms}`;
  };

  return (
    <div className="console-panel run-log-panel">
      <div className="console-header">
        <span className="console-title">Run Log</span>
        <div className="spacer" />
        <Button variant="ghost" size="xs" onClick={onClear}>
          Clear
        </Button>
        <Button variant="ghost" size="xs" onClick={onClose}>
          Close
        </Button>
      </div>
      <div className="console-body" onScroll={handleScroll}>
        {events.length === 0 && (
          <div className="console-empty">
            Waiting for run events...
          </div>
        )}
        {runIds.map((runId) => {
          const runEntries = events.filter((e) => e.run_id === runId);
          const isCollapsed = collapsedRuns.has(runId) && runId !== latestRunId;
          const startEvent = runEntries[0];
          const endEvent = runEntries.find(
            (e) =>
              e.event_type === "run_completed" ||
              e.event_type === "run_failed"
          );

          return (
            <div key={runId}>
              <div
                className="run-log-group-header"
                onClick={() => toggleRun(runId)}
              >
                <span className="run-log-chevron">
                  {isCollapsed ? "\u25b6" : "\u25bc"}
                </span>
                <span className="console-time">
                  {formatTime(startEvent.timestamp)}
                </span>
                <span
                  className="run-log-badge"
                  style={{
                    color: endEvent
                      ? EVENT_COLORS[endEvent.event_type]
                      : "var(--accent)",
                  }}
                >
                  {endEvent
                    ? endEvent.event_type === "run_completed"
                      ? "COMPLETED"
                      : "FAILED"
                    : "RUNNING"}
                </span>
                <span className="run-log-run-id">
                  {runId.slice(0, 8)}
                </span>
                {isCollapsed && (
                  <span className="run-log-summary">
                    {runEntries.length} events
                    {endEvent ? ` \u2014 ${endEvent.message}` : ""}
                  </span>
                )}
              </div>
              {!isCollapsed &&
                runEntries.map((entry, i) => (
                  <div
                    key={`${runId}-${i}`}
                    className={`console-entry run-log-entry run-log-${entry.event_type.includes("failed") ? "failed" : entry.event_type.includes("completed") ? "completed" : "default"}`}
                  >
                    <span className="console-time">
                      {formatTime(entry.timestamp)}
                    </span>
                    <span
                      className="run-log-badge"
                      style={{
                        color: EVENT_COLORS[entry.event_type] || "var(--text-secondary)",
                      }}
                    >
                      {EVENT_LABELS[entry.event_type] || entry.event_type}
                    </span>
                    <span className="console-message">
                      {entry.message}
                    </span>
                  </div>
                ))}
            </div>
          );
        })}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
