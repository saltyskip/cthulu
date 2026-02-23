import { useState, useEffect, useRef } from "react";
import * as api from "../api/client";
import { log } from "../api/logger";
import type { FlowNode } from "../types/flow";

function formatRelativeTime(iso: string): string {
  const now = Date.now();
  const target = new Date(iso).getTime();
  const diffMs = target - now;
  if (diffMs < 0) return "now";
  const diffMin = Math.round(diffMs / 60000);
  if (diffMin < 1) return "<1m";
  if (diffMin < 60) return `${diffMin}m`;
  const diffHr = Math.floor(diffMin / 60);
  const remMin = diffMin % 60;
  if (diffHr < 24) return remMin > 0 ? `${diffHr}h ${remMin}m` : `${diffHr}h`;
  const diffDays = Math.floor(diffHr / 24);
  return `${diffDays}d ${diffHr % 24}h`;
}

interface TopBarProps {
  flow: { name: string; enabled: boolean } | null;
  flowId: string | null;
  onTrigger: () => void;
  onToggleEnabled: () => void;
  onRename: (name: string) => void;
  onSettingsClick: () => void;
  consoleOpen: boolean;
  onToggleConsole: () => void;
  runLogOpen: boolean;
  onToggleRunLog: () => void;
  errorCount: number;
  flowHasErrors?: boolean;
  validationErrors?: Record<string, string[]>;
  flowNodes?: FlowNode[];
}

export default function TopBar({
  flow,
  flowId,
  onTrigger,
  onToggleEnabled,
  onRename,
  onSettingsClick,
  consoleOpen,
  onToggleConsole,
  runLogOpen,
  onToggleRunLog,
  errorCount,
  flowHasErrors,
  validationErrors,
  flowNodes,
}: TopBarProps) {
  const [connected, setConnected] = useState(false);
  const [showValidationGate, setShowValidationGate] = useState(false);
  const [nextRun, setNextRun] = useState<string | null>(null);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState("");
  const nameInputRef = useRef<HTMLInputElement>(null);

  // Auto-dismiss gate when errors are fixed
  useEffect(() => {
    if (!flowHasErrors) setShowValidationGate(false);
  }, [flowHasErrors]);

  // Fetch next run time for current flow
  useEffect(() => {
    if (!flowId || !connected) {
      setNextRun(null);
      return;
    }
    let cancelled = false;
    api.getFlowSchedule(flowId).then((info) => {
      if (!cancelled) {
        setNextRun(info.next_run ?? null);
      }
    }).catch(() => {
      if (!cancelled) setNextRun(null);
    });
    return () => { cancelled = true; };
  }, [flowId, connected, flow?.enabled]);

  useEffect(() => {
    let cancelled = false;

    // Fast retry on boot: 1s, 2s, 3s... then settle at 10s
    const check = async (interval: number) => {
      if (cancelled) return;
      const ok = await api.checkConnection();
      if (!cancelled) {
        const wasDisconnected = !connected;
        setConnected(ok);

        if (ok && wasDisconnected) {
          log("info", `Connected to server at ${api.getServerUrl()}`);
        }

        // If still disconnected, retry faster (up to 10s)
        const nextInterval = ok ? 10000 : Math.min(interval + 1000, 10000);
        retryRef.current = setTimeout(() => check(nextInterval), nextInterval);
      }
    };

    check(1000);

    return () => {
      cancelled = true;
      if (retryRef.current) clearTimeout(retryRef.current);
    };
  }, []);

  const handleRunClick = () => {
    if (flowHasErrors) {
      setShowValidationGate(true);
    } else {
      onTrigger();
    }
  };

  const handleRunAnyway = () => {
    setShowValidationGate(false);
    onTrigger();
  };

  const nodeMap = new Map((flowNodes ?? []).map((n) => [n.id, n]));

  return (
    <>
      <div className="top-bar">
        <h1>Cthulu Studio</h1>
        {flow && (
          <>
            {editing ? (
              <input
                ref={nameInputRef}
                className="flow-name-input"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                onBlur={() => {
                  const trimmed = editName.trim();
                  if (trimmed && trimmed !== flow.name) onRename(trimmed);
                  setEditing(false);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                  if (e.key === "Escape") { setEditName(flow.name); setEditing(false); }
                }}
              />
            ) : (
              <span
                className="flow-name"
                onClick={() => { setEditName(flow.name); setEditing(true); setTimeout(() => nameInputRef.current?.select(), 0); }}
                title="Click to rename"
                style={{ cursor: "text" }}
              >
                {flow.name}
              </span>
            )}
            <button
              className={`ghost flow-toggle ${flow.enabled ? "flow-toggle-enabled" : "flow-toggle-disabled"}`}
              onClick={onToggleEnabled}
            >
              <span className={`flow-toggle-dot ${flow.enabled ? "enabled" : "disabled"}`} />
              {flow.enabled ? "Enabled" : "Disabled"}
            </button>
            {nextRun && flow.enabled && (
              <span className="next-run-label" title={new Date(nextRun).toLocaleString()}>
                Next: {formatRelativeTime(nextRun)}
              </span>
            )}
          </>
        )}
        <div className="spacer" />
        {flow && (
          <button
            className="primary"
            onClick={handleRunClick}
            disabled={!connected}
            title={
              !connected
                ? "Server disconnected"
                : !(flow.enabled)
                  ? "Flow is disabled — manual run still works"
                  : undefined
            }
          >
            Run{!(flow.enabled) ? " (Manual)" : ""}
          </button>
        )}
        <div className="connection-status">
          <div
            className={`connection-dot ${connected ? "connected" : "disconnected"}`}
          />
          <span>{connected ? api.getServerUrl() : "Disconnected"}</span>
        </div>
        <button
          className={`ghost ${runLogOpen ? "console-toggle-active" : ""}`}
          onClick={onToggleRunLog}
        >
          Log
        </button>
        <button
          className={`ghost ${consoleOpen ? "console-toggle-active" : ""}`}
          onClick={onToggleConsole}
          style={{ position: "relative" }}
        >
          Console
          {errorCount > 0 && !consoleOpen && (
            <span className="error-badge">{errorCount}</span>
          )}
        </button>
        <button className="ghost" onClick={onSettingsClick}>
          Settings
        </button>
      </div>

      {showValidationGate && validationErrors && (
        <div className="validation-gate-overlay" onClick={() => setShowValidationGate(false)}>
          <div className="validation-gate" onClick={(e) => e.stopPropagation()}>
            <div className="validation-gate-header">
              Flow has validation errors
            </div>
            <div className="validation-gate-body">
              {Object.entries(validationErrors).map(([nodeId, errs]) => {
                const node = nodeMap.get(nodeId);
                return (
                  <div key={nodeId} className="validation-gate-node">
                    <strong>{node?.label ?? nodeId}</strong>
                    <span className="validation-gate-kind">{node?.kind}</span>
                    <ul>
                      {errs.map((err, i) => (
                        <li key={i}>{err}</li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
            <div className="validation-gate-footer">
              <button className="ghost" onClick={() => setShowValidationGate(false)}>
                Cancel
              </button>
              <button className="danger" onClick={handleRunAnyway}>
                Run Anyway
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
