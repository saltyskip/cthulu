import { useEffect, useRef, useState, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { startAgentChat, type InteractSSEEvent } from "../api/interactStream";
import * as api from "../api/client";

interface NodeTerminalProps {
  agentId: string;
  flowId?: string;
  nodeId?: string;
  nodeLabel: string;
  runtime?: string; // "local" | "sandbox" | "vm-sandbox"
}

// ANSI color codes
const RESET = "\x1b[0m";
const CYAN = "\x1b[36m";
const DIM = "\x1b[2m";
const GREEN = "\x1b[32m";
const RED = "\x1b[31m";
const YELLOW = "\x1b[33m";
const BOLD = "\x1b[1m";

export default function NodeTerminal({
  agentId,
  flowId,
  nodeId,
  nodeLabel,
  runtime,
}: NodeTerminalProps) {
  const termRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const inputBuffer = useRef("");
  const initializedRef = useRef(false);

  // Initialize terminal
  useEffect(() => {
    if (!termRef.current || initializedRef.current) return;
    initializedRef.current = true;

    const term = new Terminal({
      fontFamily: '"SF Mono", "Fira Code", "Cascadia Code", monospace',
      fontSize: 12,
      lineHeight: 1.4,
      theme: {
        background: "#1a1a2e",
        foreground: "#e0e0e0",
        cursor: "#e0e0e0",
        cyan: "#56d4dd",
        green: "#56dd6c",
        red: "#dd5656",
        yellow: "#ddc856",
      },
      cursorBlink: true,
      convertEol: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termRef.current);

    // Delay fit to ensure container is sized
    requestAnimationFrame(() => {
      try {
        fitAddon.fit();
      } catch {
        // Container may not be visible yet
      }
    });

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    term.writeln(`${BOLD}${CYAN}${nodeLabel}${RESET} ${DIM}(${runtime || "local"})${RESET}`);
    term.writeln(`${DIM}Type a message and press Enter to send${RESET}`);
    term.writeln("");

    // Handle user input
    term.onData((data) => {
      if (running) return; // Don't accept input while processing

      if (data === "\r") {
        // Enter pressed — send message
        const message = inputBuffer.current.trim();
        inputBuffer.current = "";
        term.write("\r\n");
        if (message) {
          sendMessage(message);
        } else {
          writePrompt(term);
        }
      } else if (data === "\x7f") {
        // Backspace
        if (inputBuffer.current.length > 0) {
          inputBuffer.current = inputBuffer.current.slice(0, -1);
          term.write("\b \b");
        }
      } else if (data >= " " || data === "\t") {
        // Printable character
        inputBuffer.current += data;
        term.write(data);
      }
    });

    writePrompt(term);

    return () => {
      term.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
      initializedRef.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Handle resize
  useEffect(() => {
    const handleResize = () => {
      try {
        fitAddonRef.current?.fit();
      } catch {
        // ignore
      }
    };
    const observer = new ResizeObserver(handleResize);
    if (termRef.current) observer.observe(termRef.current);
    return () => observer.disconnect();
  }, []);

  // Initialize session
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const sessions = await api.listAgentSessions(agentId);
        if (cancelled) return;
        if (sessions.sessions.length > 0) {
          setSessionId(sessions.active_session);
        } else {
          const result = await api.newAgentSession(agentId);
          if (!cancelled) setSessionId(result.session_id);
        }
      } catch {
        // Will create session on first message
      }
    })();
    return () => { cancelled = true; };
  }, [agentId]);

  function writePrompt(term: Terminal) {
    term.write(`${GREEN}> ${RESET}`);
  }

  const sendMessage = useCallback(
    (message: string) => {
      const term = terminalRef.current;
      if (!term || running) return;

      setRunning(true);
      term.writeln(`${BOLD}${YELLOW}You:${RESET} ${message}`);

      const handleEvent = (event: InteractSSEEvent) => {
        try {
          const parsed = JSON.parse(event.data);

          switch (event.type) {
            case "system":
              term.writeln(`${DIM}${parsed.message || ""}${RESET}`);
              break;
            case "text":
              term.write(parsed.text || "");
              break;
            case "tool_use":
              term.writeln(
                `\r\n${CYAN}[tool] ${parsed.tool}${RESET}${DIM} ${truncate(parsed.input || "", 120)}${RESET}`
              );
              break;
            case "tool_result":
              term.writeln(
                `${DIM}[result] ${truncate(parsed.content || "", 200)}${RESET}`
              );
              break;
            case "result":
              term.writeln(
                `\r\n${GREEN}${parsed.text || ""}${RESET}`
              );
              if (parsed.cost) {
                term.writeln(
                  `${DIM}(${parsed.turns || 0} turns, $${Number(parsed.cost).toFixed(4)})${RESET}`
                );
              }
              break;
            case "error":
              term.writeln(`${RED}Error: ${parsed.message || event.data}${RESET}`);
              break;
            case "cost":
              term.writeln(
                `${DIM}Cost: $${Number(parsed.total_cost || 0).toFixed(4)}${RESET}`
              );
              break;
            default:
              break;
          }
        } catch {
          // Non-JSON data
          term.writeln(event.data);
        }
      };

      const handleDone = () => {
        setRunning(false);
        abortRef.current = null;
        term.writeln("");
        writePrompt(term);
      };

      const handleError = (err: string) => {
        term.writeln(`${RED}Error: ${err}${RESET}`);
        setRunning(false);
        abortRef.current = null;
        writePrompt(term);
      };

      const flowContext = flowId && nodeId ? { flow_id: flowId, node_id: nodeId } : undefined;

      abortRef.current = startAgentChat(
        agentId,
        message,
        sessionId,
        handleEvent,
        handleDone,
        handleError,
        flowContext
      );
    },
    [agentId, flowId, nodeId, sessionId, running]
  );

  const handleNewSession = useCallback(async () => {
    try {
      const result = await api.newAgentSession(agentId);
      setSessionId(result.session_id);
      const term = terminalRef.current;
      if (term) {
        term.writeln(`\r\n${DIM}--- New session ---${RESET}\r\n`);
        writePrompt(term);
      }
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  }, [agentId]);

  const handleStop = useCallback(async () => {
    abortRef.current?.abort();
    try {
      await api.stopAgentChat(agentId, sessionId || undefined);
    } catch {
      // ignore
    }
    setRunning(false);
    const term = terminalRef.current;
    if (term) {
      term.writeln(`\r\n${RED}[stopped]${RESET}`);
      writePrompt(term);
    }
  }, [agentId, sessionId]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "2px 8px",
          borderBottom: "1px solid var(--border)",
          fontSize: 11,
          color: "var(--text-secondary)",
          flexShrink: 0,
        }}
      >
        <span>{nodeLabel}</span>
        <span style={{ opacity: 0.5 }}>|</span>
        <span>{runtime || "local"}</span>
        {running && (
          <>
            <span style={{ opacity: 0.5 }}>|</span>
            <span style={{ color: "var(--accent)" }}>running...</span>
          </>
        )}
        <div style={{ marginLeft: "auto", display: "flex", gap: 4 }}>
          {running ? (
            <button
              className="ghost"
              style={{ fontSize: 10, padding: "1px 6px" }}
              onClick={handleStop}
            >
              Stop
            </button>
          ) : (
            <button
              className="ghost"
              style={{ fontSize: 10, padding: "1px 6px" }}
              onClick={handleNewSession}
            >
              New Session
            </button>
          )}
        </div>
      </div>
      <div ref={termRef} style={{ flex: 1, minHeight: 0 }} />
    </div>
  );
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + "..." : str;
}
