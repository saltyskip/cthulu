import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { spawnPty, writePty, resizePty } from "../api/client";
import "@xterm/xterm/css/xterm.css";

interface AgentTerminalProps {
  agentId: string;
  sessionId: string;
  /** When set, skips agent lookup and spawns Claude Code in this directory. */
  workingDir?: string;
  /** When set, resolves to ~/.cthulu/cthulu-workflows/<workspace>/ on the backend. */
  workspace?: string;
  /** When set with workspace, resolves to ~/.cthulu/cthulu-workflows/<workspace>/<workflowName>/ */
  workflowName?: string;
}

/**
 * Embedded xterm.js terminal that connects to a Claude Code CLI process
 * running in a real PTY on the Rust backend.
 */
export default function AgentTerminal({
  agentId,
  sessionId,
  workingDir,
  workspace,
  workflowName,
}: AgentTerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const unlistenDataRef = useRef<UnlistenFn | null>(null);
  const unlistenExitRef = useRef<UnlistenFn | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);

  // Read CSS variable values for xterm.js theme
  const getTheme = useCallback(() => {
    const style = getComputedStyle(document.documentElement);
    const bg = style.getPropertyValue("--bg").trim();
    const text = style.getPropertyValue("--text").trim();
    const accent = style.getPropertyValue("--accent").trim();
    return {
      background: bg || "#1a1a2e",
      foreground: text || "#e0e0e0",
      cursor: accent || "#7c3aed",
      selectionBackground: (accent || "#7c3aed") + "40",
    };
  }, []);

  // Cleanup function
  const cleanup = useCallback(() => {
    unlistenDataRef.current?.();
    unlistenDataRef.current = null;
    unlistenExitRef.current?.();
    unlistenExitRef.current = null;
    resizeObserverRef.current?.disconnect();
    resizeObserverRef.current = null;
    if (terminalRef.current) {
      terminalRef.current.dispose();
      terminalRef.current = null;
    }
    fitAddonRef.current = null;
  }, []);

  useEffect(() => {
    if (!containerRef.current) return;

    // Create terminal
    const terminal = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 13,
      fontFamily:
        "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      lineHeight: 1.2,
      scrollback: 10000,
      theme: getTheme(),
      allowProposedApi: true,
      convertEol: false,
      disableStdin: false,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    terminal.loadAddon(fitAddon);
    terminal.loadAddon(webLinksAddon);
    terminal.open(containerRef.current);

    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    // Fit to container
    requestAnimationFrame(() => {
      fitAddon.fit();
    });

    // Forward user input to PTY
    terminal.onData((data: string) => {
      writePty(sessionId, data).catch((err) => {
        console.error("write_pty error:", err);
      });
    });

    // Forward resize events to PTY
    terminal.onResize(
      ({ cols, rows }: { cols: number; rows: number }) => {
        resizePty(sessionId, cols, rows).catch((err) => {
          console.error("resize_pty error:", err);
        });
      },
    );

    // Watch container for size changes
    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        fitAddonRef.current?.fit();
      });
    });
    observer.observe(containerRef.current);
    resizeObserverRef.current = observer;

    // Spawn PTY and subscribe to output
    let cancelled = false;

    (async () => {
      try {
        // Spawn the PTY (idempotent — returns existing if already running)
        await spawnPty(agentId, sessionId, {
          workingDirOverride: workingDir,
          workspace,
          workflowName,
        });

        if (cancelled) return;

        // Listen for PTY output
        const unlistenData = await listen<string>(
          `pty-data-${sessionId}`,
          (event) => {
            terminal.write(event.payload);
          },
        );
        unlistenDataRef.current = unlistenData;

        // Listen for PTY exit
        const unlistenExit = await listen<{ session_id: string }>(
          `pty-exit-${sessionId}`,
          () => {
            terminal.write(
              "\r\n\x1b[90m[Session ended. Press Enter to restart.]\x1b[0m\r\n",
            );
            // On next Enter, restart the PTY
            const disposable = terminal.onData((data: string) => {
              if (data === "\r" || data === "\n") {
                disposable.dispose();
                terminal.clear();
                terminal.write("Restarting session...\r\n");
                spawnPty(agentId, sessionId, {
                          workingDirOverride: workingDir,
                          workspace,
                          workflowName,
                        })
                  .then(() => {
                    terminal.write("Session restarted.\r\n");
                  })
                  .catch((err) => {
                    terminal.write(
                      `\r\n\x1b[31mFailed to restart: ${err}\x1b[0m\r\n`,
                    );
                  });
              }
            });
          },
        );
        unlistenExitRef.current = unlistenExit;

        // Send initial resize after connection
        const { cols, rows } = terminal;
        await resizePty(sessionId, cols, rows);
      } catch (err) {
        const msg =
          typeof err === "string"
            ? err
            : err instanceof Error
              ? err.message
              : String(err);
        terminal.write(
          `\r\n\x1b[31mFailed to start terminal: ${msg}\x1b[0m\r\n`,
        );
        if (msg.includes("not found") || msg.includes("No such file")) {
          terminal.write(
            "\x1b[33mMake sure Claude Code CLI is installed: brew install claude-code\x1b[0m\r\n",
          );
        }
      }
    })();

    return () => {
      cancelled = true;
      cleanup();
    };
  }, [agentId, sessionId, workingDir, workspace, workflowName, getTheme, cleanup]);

  return (
    <div
      ref={containerRef}
      className="agent-terminal"
      style={{
        width: "100%",
        height: "100%",
        overflow: "hidden",
      }}
    />
  );
}
