import { useState, useEffect, useRef, useCallback } from "react";
import * as api from "../api/client";
import { log } from "../api/logger";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTheme } from "@/lib/ThemeContext";
import { themes } from "@/lib/themes";

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

type ActiveView = "flow-editor" | "agent-grid" | "agent-workspace" | "prompt-editor";

interface TopBarProps {
  activeView: ActiveView;
  flow: { name: string; enabled: boolean } | null;
  flowId: string | null;
  onTrigger: () => void;
  onRename: (name: string) => void;
  agentName: string | null;
  onBackToFlow: () => void;
  onShowAgentGrid?: () => void;
  onSettingsClick: () => void;
  onReconnect?: () => void;
}

export default function TopBar({
  activeView,
  flow,
  flowId,
  onTrigger,
  onRename,
  agentName,
  onBackToFlow,
  onShowAgentGrid,
  onSettingsClick,
  onReconnect,
}: TopBarProps) {
  const [connected, setConnected] = useState(false);
  const [nextRun, setNextRun] = useState<string | null>(null);
  const [tokenOk, setTokenOk] = useState<boolean | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const connectedRef = useRef(false);
  const onReconnectRef = useRef(onReconnect);
  onReconnectRef.current = onReconnect;
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState("");
  const nameInputRef = useRef<HTMLInputElement>(null);

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

    const check = async (interval: number) => {
      if (cancelled) return;
      const ok = await api.checkConnection();
      if (!cancelled) {
        const wasDisconnected = !connectedRef.current;
        connectedRef.current = ok;
        setConnected(ok);

        if (ok && wasDisconnected) {
          log("info", `Connected to server at ${api.getServerUrl()}`);
          onReconnectRef.current?.();
        }

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

  // Check token status whenever we connect
  useEffect(() => {
    if (!connected) return;
    api.getTokenStatus()
      .then((res) => setTokenOk(res.has_token))
      .catch(() => setTokenOk(null));
  }, [connected]);

  const handleRefreshToken = useCallback(async () => {
    setRefreshing(true);
    try {
      const res = await api.refreshToken();
      if (res.ok) {
        setTokenOk(true);
        log("info", "OAuth token refreshed successfully");
      } else {
        log("warn", `Token refresh failed: ${res.message}`);
        alert(`Token refresh failed:\n\n${res.message}`);
      }
    } catch (e) {
      log("error", `Token refresh error: ${(e as Error).message}`);
    } finally {
      setRefreshing(false);
    }
  }, []);

  return (
    <div className="top-bar">
      <h1>Cthulu Studio</h1>

      {activeView === "agent-grid" && (
        <>
          <Button variant="ghost" size="sm" className="top-bar-back" onClick={onBackToFlow}>
            ← Back
          </Button>
          <span className="top-bar-agent-name">Agents</span>
        </>
      )}

      {activeView === "agent-workspace" && (
        <Button variant="ghost" size="sm" className="top-bar-back" onClick={onShowAgentGrid || onBackToFlow}>
          ← Agents
        </Button>
      )}

      {activeView === "prompt-editor" && (
        <>
          <Button variant="ghost" size="sm" className="top-bar-back" onClick={onBackToFlow}>
            ← Back
          </Button>
          <span className="top-bar-agent-name">Prompt Editor</span>
        </>
      )}

      {activeView === "flow-editor" && flow && (
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
          {flow.enabled && (
            <span className="flow-enabled-badge">Active</span>
          )}
          {nextRun && flow.enabled && (
            <span className="next-run-label" title={new Date(nextRun).toLocaleString()}>
              Next: {formatRelativeTime(nextRun)}
            </span>
          )}
        </>
      )}

      {activeView === "agent-workspace" && agentName && (
        <span className="top-bar-agent-name">{agentName}</span>
      )}

      <div className="spacer" />

      {activeView === "flow-editor" && flow && (
        <Button
          size="sm"
          onClick={onTrigger}
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
        </Button>
      )}

      <div className="connection-status">
        <div
          className={`connection-dot ${connected ? "connected" : "disconnected"}`}
          title={connected ? api.getServerUrl() : "Disconnected"}
        />
      </div>

      {tokenOk === false && (
        <button
          className="topbar-token-btn expired"
          onClick={handleRefreshToken}
          disabled={refreshing || !connected}
          title="OAuth token expired — click to refresh"
        >
          {refreshing ? "Refreshing…" : "⚠ Token Expired"}
        </button>
      )}

      <ThemeSelector />

      <Button variant="ghost" size="sm" onClick={onSettingsClick}>
        Settings
      </Button>
    </div>
  );
}

function ThemeSelector() {
  const { theme, setThemeId } = useTheme();
  const branded = themes.filter((t) => t.group === "branded");
  const presets = themes.filter((t) => t.group === "preset");

  return (
    <Select value={theme.id} onValueChange={setThemeId}>
      <SelectTrigger size="sm" className="text-xs h-7 min-w-[120px]">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectLabel>Branded</SelectLabel>
          {branded.map((t) => (
            <SelectItem key={t.id} value={t.id} className="text-xs">
              {t.label}
            </SelectItem>
          ))}
        </SelectGroup>
        <SelectSeparator />
        <SelectGroup>
          <SelectLabel>Presets</SelectLabel>
          {presets.map((t) => (
            <SelectItem key={t.id} value={t.id} className="text-xs">
              {t.label}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  );
}
