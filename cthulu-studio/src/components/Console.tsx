import { useState, useEffect, useRef } from "react";
import { getEntries, clearEntries, subscribe, type LogEntry, type LogLevel } from "../api/logger";

const LEVEL_COLORS: Record<LogLevel, string> = {
  info: "var(--accent)",
  warn: "var(--warning)",
  error: "var(--danger)",
  http: "var(--text-secondary)",
};

const LEVEL_LABELS: Record<LogLevel, string> = {
  info: "INFO",
  warn: "WARN",
  error: "ERR ",
  http: "HTTP",
};

interface ConsoleProps {
  onClose: () => void;
}

export default function Console({ onClose }: ConsoleProps) {
  const [entries, setEntries] = useState<LogEntry[]>(getEntries());
  const [filter, setFilter] = useState<LogLevel | "all">("all");
  const bottomRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    return subscribe(() => {
      setEntries([...getEntries()]);
    });
  }, []);

  useEffect(() => {
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [entries, autoScroll]);

  const filtered =
    filter === "all" ? entries : entries.filter((e) => e.level === filter);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  };

  return (
    <div className="console-panel">
      <div className="console-header">
        <span className="console-title">Console</span>
        <div className="console-filters">
          {(["all", "http", "info", "warn", "error"] as const).map((level) => (
            <button
              key={level}
              className={`console-filter ${filter === level ? "active" : ""}`}
              onClick={() => setFilter(level)}
            >
              {level.toUpperCase()}
              {level !== "all" && (
                <span className="console-filter-count">
                  {entries.filter((e) => e.level === level).length}
                </span>
              )}
            </button>
          ))}
        </div>
        <div className="spacer" />
        <button className="ghost console-btn" onClick={() => clearEntries()}>
          Clear
        </button>
        <button className="ghost console-btn" onClick={onClose}>
          Close
        </button>
      </div>
      <div className="console-body" onScroll={handleScroll}>
        {filtered.length === 0 && (
          <div className="console-empty">No log entries</div>
        )}
        {filtered.map((entry) => (
          <div key={entry.id} className={`console-entry console-${entry.level}`}>
            <span className="console-time">
              {entry.timestamp.toLocaleTimeString("en-US", {
                hour12: false,
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })}
              .{String(entry.timestamp.getMilliseconds()).padStart(3, "0")}
            </span>
            <span
              className="console-level"
              style={{ color: LEVEL_COLORS[entry.level] }}
            >
              {LEVEL_LABELS[entry.level]}
            </span>
            <span className="console-message">{entry.message}</span>
            {entry.detail && (
              <span className="console-detail">{entry.detail}</span>
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
