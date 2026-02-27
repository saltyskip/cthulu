import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { AttachAddon } from "@xterm/addon-attach";
import "@xterm/xterm/css/xterm.css";
import { Button } from "@/components/ui/button";
import { getTerminalWsUrl } from "../api/client";

interface NodeTerminalProps {
  agentId: string;
  flowId?: string;
  nodeId?: string;
  nodeLabel: string;
  runtime?: string; // "local" | "sandbox" | "vm-sandbox"
}

export default function NodeTerminal({
  agentId,
  nodeLabel,
  runtime,
}: NodeTerminalProps) {
  const termRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const initializedRef = useRef(false);

  // Initialize terminal + WebSocket
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
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termRef.current);

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    // Delay fit + WS connect to ensure container is sized
    requestAnimationFrame(() => {
      try {
        fitAddon.fit();
      } catch {
        // Container may not be visible yet
      }
      connectWs(term, fitAddon);
    });

    return () => {
      wsRef.current?.close();
      wsRef.current = null;
      term.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
      initializedRef.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function connectWs(term: Terminal, fitAddon: FitAddon) {
    const url = getTerminalWsUrl(agentId);
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    wsRef.current = ws;

    ws.onopen = () => {
      // Attach addon handles bidirectional binary I/O
      const attachAddon = new AttachAddon(ws);
      term.loadAddon(attachAddon);

      // Send initial resize
      try {
        fitAddon.fit();
      } catch {
        // ignore
      }
      sendResize(ws, term.cols, term.rows);
    };

    ws.onclose = () => {
      term.writeln("\r\n\x1b[2m[disconnected]\x1b[0m");
    };

    ws.onerror = () => {
      term.writeln("\r\n\x1b[31m[connection error]\x1b[0m");
    };
  }

  function sendResize(ws: WebSocket, cols: number, rows: number) {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "resize", cols, rows }));
    }
  }

  // Handle resize
  useEffect(() => {
    const handleResize = () => {
      try {
        fitAddonRef.current?.fit();
      } catch {
        // ignore
      }
      const term = terminalRef.current;
      const ws = wsRef.current;
      if (term && ws) {
        sendResize(ws, term.cols, term.rows);
      }
    };
    const observer = new ResizeObserver(handleResize);
    if (termRef.current) observer.observe(termRef.current);
    return () => observer.disconnect();
  }, []);

  const handleStop = useCallback(() => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      // Send Ctrl+C as binary
      const ctrlC = new Uint8Array([0x03]);
      ws.send(ctrlC);
    }
  }, []);

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
        <div style={{ marginLeft: "auto", display: "flex", gap: 4 }}>
          <Button
            variant="ghost"
            size="xs"
            onClick={handleStop}
          >
            Stop
          </Button>
        </div>
      </div>
      <div ref={termRef} style={{ flex: 1, minHeight: 0 }} />
    </div>
  );
}
