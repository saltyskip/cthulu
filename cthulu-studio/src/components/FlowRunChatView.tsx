import { useState, useEffect, useRef, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { getSessionLog, streamSessionLog } from "../api/client";
import type { FlowRunMeta } from "../api/client";

interface FlowRunChatViewProps {
  agentId: string;
  sessionId: string;
  busy: boolean;
  flowRun?: FlowRunMeta;
}

// Parsed event types from stream-json
type ParsedEvent =
  | { type: "system"; data: Record<string, unknown> }
  | { type: "assistant_text"; text: string }
  | { type: "tool_use"; tool: string; input: string }
  | { type: "tool_result"; content: string; is_error: boolean }
  | {
      type: "result";
      text: string;
      cost: number;
      turns: number;
    }
  | { type: "raw"; line: string };

function parseLine(raw: string): ParsedEvent | null {
  if (!raw.trim()) return null;

  try {
    const obj = JSON.parse(raw);
    const eventType = obj.type;

    if (eventType === "system") {
      return { type: "system", data: obj };
    }

    if (eventType === "assistant") {
      const content = obj.message?.content;
      if (Array.isArray(content)) {
        // Extract text blocks
        const textParts: string[] = [];
        const events: ParsedEvent[] = [];
        for (const block of content) {
          if (block.type === "text" && block.text) {
            textParts.push(block.text);
          } else if (block.type === "tool_use") {
            events.push({
              type: "tool_use",
              tool: block.name || "?",
              input:
                typeof block.input === "string"
                  ? block.input
                  : JSON.stringify(block.input, null, 2),
            });
          } else if (block.type === "tool_result") {
            events.push({
              type: "tool_result",
              content:
                typeof block.content === "string"
                  ? block.content
                  : JSON.stringify(block.content),
              is_error: !!block.is_error,
            });
          }
        }
        // Return first meaningful event (text takes priority)
        if (textParts.length > 0) {
          return { type: "assistant_text", text: textParts.join("\n") };
        }
        if (events.length > 0) return events[0];
      }
      return null;
    }

    if (eventType === "result") {
      return {
        type: "result",
        text: obj.result || "",
        cost: obj.total_cost_usd || 0,
        turns: obj.num_turns || 0,
      };
    }

    // content_block_start with tool_use
    if (eventType === "content_block_start" && obj.content_block?.type === "tool_use") {
      return {
        type: "tool_use",
        tool: obj.content_block.name || "?",
        input: "",
      };
    }

    return null;
  } catch {
    return { type: "raw", line: raw };
  }
}

export default function FlowRunChatView({
  agentId,
  sessionId,
  busy,
  flowRun,
}: FlowRunChatViewProps) {
  const [events, setEvents] = useState<ParsedEvent[]>([]);
  const [isLive, setIsLive] = useState(busy);
  const scrollRef = useRef<HTMLDivElement>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  const addLine = useCallback((raw: string) => {
    const parsed = parseLine(raw);
    if (parsed) {
      setEvents((prev) => [...prev, parsed]);
    }
  }, []);

  useEffect(() => {
    setEvents([]);
    setIsLive(busy);

    if (busy) {
      // Live: use SSE stream (which replays + subscribes)
      const cleanup = streamSessionLog(
        agentId,
        sessionId,
        addLine,
        () => setIsLive(false)
      );
      cleanupRef.current = cleanup;
      return cleanup;
    } else {
      // Completed: fetch full log
      (async () => {
        try {
          const lines = await getSessionLog(agentId, sessionId);
          const parsed = lines
            .map(parseLine)
            .filter((e): e is ParsedEvent => e !== null);
          setEvents(parsed);
        } catch {
          // ignore
        }
      })();
    }
  }, [agentId, sessionId, busy, addLine]);

  // Auto-scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [events]);

  // Collapse state for tool blocks
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const toggleCollapse = useCallback((idx: number) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) {
        next.delete(idx);
      } else {
        next.add(idx);
      }
      return next;
    });
  }, []);

  return (
    <div className="flow-run-chat" ref={scrollRef}>
      {flowRun && (
        <div className="flow-run-header">
          <span className="flow-run-header-label">
            {flowRun.flow_name} &mdash; {flowRun.node_label}
          </span>
          <span className="flow-run-header-run">Run: {flowRun.run_id.slice(0, 8)}</span>
        </div>
      )}

      <div className="flow-run-messages">
        {events.map((evt, i) => {
          switch (evt.type) {
            case "system":
              return null;

            case "assistant_text":
              return (
                <div key={i} className="flow-run-message">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {evt.text}
                  </ReactMarkdown>
                </div>
              );

            case "tool_use":
              return (
                <div key={i} className="flow-run-tool">
                  <div
                    className="flow-run-tool-header"
                    onClick={() => toggleCollapse(i)}
                  >
                    <span className="flow-run-tool-icon">
                      {collapsed.has(i) ? "▶" : "▼"}
                    </span>
                    <span className="flow-run-tool-name">{evt.tool}</span>
                  </div>
                  {!collapsed.has(i) && evt.input && (
                    <div className="flow-run-tool-body">
                      <pre>{evt.input}</pre>
                    </div>
                  )}
                </div>
              );

            case "tool_result":
              return (
                <div
                  key={i}
                  className={`flow-run-tool ${evt.is_error ? "flow-run-tool-error" : ""}`}
                >
                  <div
                    className="flow-run-tool-header"
                    onClick={() => toggleCollapse(i)}
                  >
                    <span className="flow-run-tool-icon">
                      {collapsed.has(i) ? "▶" : "▼"}
                    </span>
                    <span className="flow-run-tool-name">
                      {evt.is_error ? "Error" : "Result"}
                    </span>
                  </div>
                  {!collapsed.has(i) && (
                    <div className="flow-run-tool-body">
                      <pre>{evt.content}</pre>
                    </div>
                  )}
                </div>
              );

            case "result":
              return (
                <div key={i} className="flow-run-result">
                  <span className="flow-run-result-badge">
                    {evt.turns} turns &middot; ${evt.cost.toFixed(4)}
                  </span>
                </div>
              );

            case "raw":
              return (
                <div key={i} className="flow-run-message">
                  <pre>{evt.line}</pre>
                </div>
              );

            default:
              return null;
          }
        })}

        {isLive && (
          <div className="flow-run-busy">
            <span className="flow-run-busy-dot" />
            Running...
          </div>
        )}
      </div>
    </div>
  );
}
