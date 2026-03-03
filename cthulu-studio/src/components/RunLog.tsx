import { useState, useEffect, useRef, useMemo } from "react";
import type { RunEvent } from "../types/flow";
import { Eraser, ArrowDownToLine } from "lucide-react";

type LogFilter = "all" | "errors" | "info";

const EVENT_COLORS: Record<string, string> = {
  run_started: "var(--accent)",
  node_started: "var(--accent)",
  node_completed: "var(--success)",
  node_failed: "var(--danger)",
  run_completed: "var(--success)",
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

export default function RunLog({ events, onClear }: RunLogProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filter, setFilter] = useState<LogFilter>("all");

  const errorCount = useMemo(() => events.filter((e) => e.event_type.includes("failed")).length, [events]);
  const infoCount = useMemo(() => events.filter((e) => !e.event_type.includes("failed")).length, [events]);

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

  const scrollToBottom = () => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    setAutoScroll(true);
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
    <div className="run-log-panel">
      {/* Filter bar + actions — VS Code style */}
      <div className="run-log-toolbar">
        <div className="run-log-filters">
          <button
            className={`run-log-filter-btn${filter === "all" ? " active" : ""}`}
            onClick={() => setFilter("all")}
          >
            All
            <span className="run-log-filter-count">{events.length}</span>
          </button>
          <button
            className={`run-log-filter-btn run-log-filter-btn--error${filter === "errors" ? " active" : ""}`}
            onClick={() => setFilter("errors")}
          >
            Errors
            <span className="run-log-filter-count">{errorCount}</span>
          </button>
          <button
            className={`run-log-filter-btn run-log-filter-btn--info${filter === "info" ? " active" : ""}`}
            onClick={() => setFilter("info")}
          >
            Info
            <span className="run-log-filter-count">{infoCount}</span>
          </button>
        </div>
        <div className="run-log-actions">
          <button className="run-log-action-btn" onClick={onClear} title="Clear output">
            <Eraser size={13} />
          </button>
          <button className="run-log-action-btn" onClick={scrollToBottom} title="Scroll to bottom">
            <ArrowDownToLine size={13} />
          </button>
        </div>
      </div>

      <div className="run-log-body" onScroll={handleScroll}>
        {events.length === 0 && (
          <div className="run-log-empty">
            <span className="run-log-empty-icon">$</span>
            <span>No output yet. Run a flow to see events here.</span>
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
            <div key={runId} className="run-log-group">
              <div
                className="run-log-group-header"
                onClick={() => toggleRun(runId)}
              >
                <span className="run-log-chevron">
                  {isCollapsed ? "\u25b6" : "\u25bc"}
                </span>
                <span className="run-log-ts">
                  {formatTime(startEvent.timestamp)}
                </span>
                <span
                  className="run-log-status"
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
                runEntries.filter((entry) => {
                  if (filter === "all") return true;
                  if (filter === "errors") return entry.event_type.includes("failed");
                  return !entry.event_type.includes("failed");
                }).map((entry, i) => (
                  <div
                    key={`${runId}-${i}`}
                    className={`run-log-line run-log-line--${entry.event_type.includes("failed") ? "error" : entry.event_type.includes("completed") ? "success" : "info"}`}
                  >
                    <span className="run-log-ts">
                      {formatTime(entry.timestamp)}
                    </span>
                    <span
                      className="run-log-label"
                      style={{
                        color: EVENT_COLORS[entry.event_type] || "var(--text-secondary)",
                      }}
                    >
                      {EVENT_LABELS[entry.event_type] || entry.event_type}
                    </span>
                    <span className="run-log-msg">
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
