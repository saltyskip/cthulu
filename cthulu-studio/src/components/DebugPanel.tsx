import { useState, useRef, useEffect, useMemo } from "react";
import type { DebugEvent } from "./chat/useAgentChat";

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

function DebugEventRow({ ev }: { ev: DebugEvent }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div className={`fr-debug-event ${ev.error ? "fr-debug-event-error" : ""}`}>
      <div className="fr-debug-event-row" onClick={() => setExpanded((v) => !v)}>
        <span className="fr-debug-expand">{expanded ? "▾" : "▸"}</span>
        <span className="fr-debug-ts">{new Date(ev.ts).toLocaleTimeString()}</span>
        <span className={`fr-debug-badge fr-debug-badge-${ev.type}`}>{ev.type}</span>
        {!expanded && (
          <span className="fr-debug-preview">
            {ev.data.length > 80 ? ev.data.slice(0, 80) + "..." : ev.data}
          </span>
        )}
      </div>
      {expanded && (
        <pre className="fr-debug-json">{prettyJson(ev.data)}</pre>
      )}
    </div>
  );
}

type DebugFilter = "all" | "chat" | "hook";

interface DebugPanelProps {
  chatEvents: DebugEvent[];
  hookEvents: DebugEvent[];
  onClearChat: () => void;
  onClearHook: () => void;
}

export default function DebugPanel({ chatEvents, hookEvents, onClearChat, onClearHook }: DebugPanelProps) {
  const [filter, setFilter] = useState<DebugFilter>("all");
  const scrollRef = useRef<HTMLDivElement>(null);

  const merged = useMemo(() => {
    let events: DebugEvent[];
    if (filter === "chat") events = chatEvents;
    else if (filter === "hook") events = hookEvents;
    else events = [...chatEvents, ...hookEvents].sort((a, b) => a.ts - b.ts);
    return events;
  }, [chatEvents, hookEvents, filter]);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [merged]);

  return (
    <div className="debug-panel">
      <div className="debug-panel-header">
        <span className="debug-panel-title">Debug</span>
        <div className="debug-panel-filters">
          {(["all", "chat", "hook"] as const).map((f) => (
            <button
              key={f}
              className={`debug-filter-btn ${filter === f ? "debug-filter-active" : ""}`}
              onClick={() => setFilter(f)}
            >
              {f === "all" ? "All" : f === "chat" ? "Chat SSE" : "Hooks"}
            </button>
          ))}
        </div>
        <button
          className="debug-panel-clear"
          onClick={() => { onClearChat(); onClearHook(); }}
          title="Clear all"
        >
          Clear
        </button>
      </div>
      <div className="debug-panel-scroll" ref={scrollRef}>
        {merged.length === 0 ? (
          <div className="debug-panel-empty">No events yet</div>
        ) : (
          merged.map((ev, i) => <DebugEventRow key={`${ev.ts}-${i}`} ev={ev} />)
        )}
      </div>
    </div>
  );
}
