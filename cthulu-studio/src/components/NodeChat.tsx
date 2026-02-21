import { useState, useEffect, useRef, useCallback } from "react";
import * as api from "../api/client";
import { startNodeInteract } from "../api/interactStream";
import { log } from "../api/logger";
import type { SessionInfo, OutputLine } from "../types/flow";

// Persisted state for a single executor node chat
export interface NodeChatState {
  session: SessionInfo | null;
  sessionId: string | null;
  prompt: string;
  outputLines: OutputLine[];
  running: boolean;
}

interface NodeChatProps {
  flowId: string;
  nodeId: string;
  nodeLabel: string;
  initialState: NodeChatState | null;
  onStateChange: (state: NodeChatState) => void;
}

function lineClass(type: OutputLine["type"]) {
  switch (type) {
    case "system":
      return "interact-line interact-line-system";
    case "text":
      return "interact-line interact-line-text";
    case "tool_use":
      return "interact-line interact-line-tool";
    case "tool_result":
      return "interact-line interact-line-tool-result";
    case "result":
      return "interact-line interact-line-result";
    case "error":
      return "interact-line interact-line-error";
    case "cost":
      return "interact-line interact-line-cost";
    default:
      return "interact-line";
  }
}

function linePrefix(type: OutputLine["type"]) {
  switch (type) {
    case "tool_use":
      return "\u2699 ";
    case "tool_result":
      return "  \u2192 ";
    case "result":
      return "\u2713 ";
    case "error":
      return "\u2717 ";
    case "cost":
      return "$ ";
    default:
      return "";
  }
}

export default function NodeChat({
  flowId,
  nodeId,
  nodeLabel,
  initialState,
  onStateChange,
}: NodeChatProps) {
  const [session, setSession] = useState<SessionInfo | null>(
    initialState?.session ?? null
  );
  const [sessionId, setSessionId] = useState<string | null>(
    initialState?.sessionId ?? null
  );
  const [prompt, setPrompt] = useState(initialState?.prompt ?? "");
  const [outputLines, setOutputLines] = useState<OutputLine[]>(
    initialState?.outputLines ?? []
  );
  const [running, setRunning] = useState(false);
  const [loading, setLoading] = useState(false);

  const [inputHeight, setInputHeight] = useState(32);

  const abortRef = useRef<AbortController | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);
  const inputDragRef = useRef<{ startY: number; startH: number } | null>(null);

  // State ref for unmount save
  const stateRef = useRef<NodeChatState>({
    session,
    sessionId,
    prompt,
    outputLines,
    running: false,
  });
  stateRef.current = { session, sessionId, prompt, outputLines, running: false };

  useEffect(() => {
    return () => {
      onStateChange(stateRef.current);
    };
  }, [onStateChange]);

  // Auto-scroll output
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [outputLines]);

  // Load or create the single session for this node on mount
  useEffect(() => {
    // Skip if we already have a session from saved state
    if (initialState?.sessionId && initialState.outputLines.length > 0) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);

    const init = async () => {
      try {
        const sess = await api.getNodeSession(flowId, nodeId);
        if (cancelled) return;
        setSession(sess);

        const info = await api.listNodeInteractSessions(flowId, nodeId);
        if (cancelled) return;

        if (info.sessions.length > 0) {
          // Reuse existing session
          const sid = info.active_session || info.sessions[0].session_id;
          setSessionId(sid);
          if (outputLines.length === 0) {
            setOutputLines([
              { type: "system", text: `${nodeLabel} — ${sess.working_dir}` },
              { type: "system", text: "Send a message to chat with this executor." },
            ]);
          }
        } else {
          // Create first session
          const newSess = await api.newNodeInteractSession(flowId, nodeId);
          if (cancelled) return;
          setSessionId(newSess.session_id);
          setOutputLines([
            { type: "system", text: `${nodeLabel} — ${sess.working_dir}` },
            { type: "system", text: "Type your message to start." },
          ]);
        }
      } catch (err) {
        if (!cancelled) {
          setOutputLines([
            { type: "error", text: `Failed to load: ${(err as Error).message}` },
          ]);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    init();
    return () => { cancelled = true; };
  }, [flowId, nodeId]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSend = useCallback(() => {
    if (!sessionId || running) return;
    const promptText = prompt.trim();
    if (!promptText) return;

    setPrompt("");
    setRunning(true);
    setOutputLines((prev) => [
      ...prev,
      { type: "system", text: `> ${promptText.length > 200 ? promptText.slice(0, 200) + "..." : promptText}` },
    ]);

    log("info", `Sending node message: flow=${flowId}, node=${nodeId}`);

    const controller = startNodeInteract(
      flowId,
      nodeId,
      promptText,
      sessionId,
      (event) => {
        try {
          const parsed = JSON.parse(event.data);
          switch (event.type) {
            case "system":
              setOutputLines((prev) => [...prev, { type: "system", text: parsed.message || "System event" }]);
              break;
            case "text":
              setOutputLines((prev) => [...prev, { type: "text", text: parsed.text || "" }]);
              break;
            case "tool_use": {
              const input = (parsed.input || "").length > 300 ? parsed.input.slice(0, 300) + "..." : parsed.input || "";
              setOutputLines((prev) => [...prev, { type: "tool_use", text: `${parsed.tool}: ${input}` }]);
              break;
            }
            case "tool_result": {
              const content = (parsed.content || "").length > 500 ? parsed.content.slice(0, 500) + "..." : parsed.content || "";
              setOutputLines((prev) => [...prev, { type: "tool_result" as OutputLine["type"], text: content }]);
              break;
            }
            case "result":
              setOutputLines((prev) => [
                ...prev,
                { type: "result", text: parsed.text || "" },
                { type: "cost", text: `Cost: $${(parsed.cost || 0).toFixed(4)} | Turns: ${parsed.turns || 0}` },
              ]);
              break;
            case "error":
              setOutputLines((prev) => [...prev, { type: "error", text: parsed.message || event.data }]);
              break;
          }
        } catch {
          setOutputLines((prev) => [...prev, { type: "text", text: event.data }]);
        }
      },
      () => {
        setRunning(false);
        log("info", "Node interact message completed");
      },
      (err) => {
        setRunning(false);
        setOutputLines((prev) => [
          ...prev,
          {
            type: err.includes("409") ? "system" : "error",
            text: err.includes("409") ? "Processing previous message... please wait." : `Stream error: ${err}`,
          },
        ]);
      }
    );

    abortRef.current = controller;
  }, [sessionId, running, prompt, flowId, nodeId]);

  const handleStop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (sessionId) {
      api.stopNodeInteract(flowId, nodeId, sessionId).catch(() => {});
    }
    setRunning(false);
    setOutputLines((prev) => [...prev, { type: "system", text: "Stopped." }]);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInputDragStart = (e: React.MouseEvent) => {
    e.preventDefault();
    inputDragRef.current = { startY: e.clientY, startH: inputHeight };
    const onMove = (ev: MouseEvent) => {
      if (!inputDragRef.current) return;
      // Dragging up = larger input (startY - ev.clientY is positive when moving up)
      const delta = inputDragRef.current.startY - ev.clientY;
      const newH = Math.min(300, Math.max(32, inputDragRef.current.startH + delta));
      setInputHeight(newH);
    };
    const onUp = () => {
      inputDragRef.current = null;
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "ns-resize";
  };

  return (
    <div className="node-chat">
      <div className="node-chat-output" ref={outputRef}>
        {loading && (
          <div className="interact-line interact-line-system">Loading session...</div>
        )}
        {outputLines.map((line, i) => (
          <div key={i} className={lineClass(line.type)}>
            {linePrefix(line.type)}
            {line.text}
          </div>
        ))}
      </div>
      <div className="node-chat-input-drag" onMouseDown={handleInputDragStart} />
      <div className="node-chat-input">
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={running ? "Waiting for response..." : "Message... (Ctrl+Enter)"}
          disabled={running || loading}
          style={{ height: inputHeight, maxHeight: 300 }}
        />
        {running ? (
          <button className="danger node-chat-send" onClick={handleStop}>Stop</button>
        ) : (
          <button
            className="primary node-chat-send"
            onClick={handleSend}
            disabled={!prompt.trim() || loading}
          >
            Send
          </button>
        )}
      </div>
    </div>
  );
}
