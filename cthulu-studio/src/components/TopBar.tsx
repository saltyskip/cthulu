import { useState, useEffect, useRef, useCallback, useMemo } from "react";
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
import type { ActiveView } from "../types/flow";
import { listen } from "@tauri-apps/api/event";
import { ChevronDown, Search, X, Check } from "lucide-react";

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
  activeView: ActiveView;
  flow: { name: string; enabled: boolean } | null;
  flowId: string | null;
  onTrigger: () => void;
  onRename: (name: string) => void;
  agentName: string | null;
  sessionSummary?: string | null;
  onBackToFlow: () => void;
  onSettingsClick: () => void;
  onReconnect?: () => void;
  onNavigate?: (view: ActiveView) => void;
  onPublish?: () => void;
  onSaveWorkflow?: () => void;
  onRunWorkflow?: () => void;
  editingWorkflow?: { workspace: string; name: string } | null;
  workspaces?: string[];
  activeWorkspace?: string | null;
  onSelectWorkspace?: (ws: string) => void;
  onCreateWorkspace?: () => void;
}

export default function TopBar({
  activeView,
  flow,
  flowId,
  onTrigger,
  onRename,
  agentName,
  sessionSummary,
  onBackToFlow,
  onSettingsClick,
  onReconnect,
  onNavigate,
  onPublish,
  onSaveWorkflow,
  onRunWorkflow,
  editingWorkflow,
  workspaces,
  activeWorkspace,
  onSelectWorkspace,
  onCreateWorkspace,
}: TopBarProps) {
  // Sync status from backend
  const [syncStatus, setSyncStatus] = useState<{ status: string; message: string } | null>(null);

  useEffect(() => {
    const unlisten = listen<{ status: string; workspace: string; message: string }>("sync-status", (event) => {
      setSyncStatus({ status: event.payload.status, message: event.payload.message });
      if (event.payload.status === "synced") {
        setTimeout(() => setSyncStatus(null), 3000);
      } else if (event.payload.status === "error") {
        setTimeout(() => setSyncStatus(null), 5000);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // In Tauri desktop mode, we're always connected via IPC
  const connected = true;
  const [nextRun, setNextRun] = useState<string | null>(null);
  const [tokenOk, setTokenOk] = useState<boolean | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState("");
  const nameInputRef = useRef<HTMLInputElement>(null);

  // Call onReconnect once on mount (desktop is always connected)
  const onReconnectRef = useRef(onReconnect);
  onReconnectRef.current = onReconnect;
  useEffect(() => {
    onReconnectRef.current?.();
  }, []);

  // Fetch next run time for current flow
  useEffect(() => {
    if (!flowId) {
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
  }, [flowId, flow?.enabled]);

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

      <div className="top-bar-nav">
        <button
          className={`top-bar-nav-item${activeView === "agent-list" || activeView === "agent-detail" || activeView === "org-chart" ? " active" : ""}`}
          onClick={() => onNavigate?.("agent-list")}
        >
          Agents
        </button>
        <button
          className={`top-bar-nav-item${activeView === "workflows" ? " active" : ""}`}
          onClick={() => onNavigate?.("workflows")}
        >
          Workflows
        </button>
      </div>

      {activeView === "workflows" && !editingWorkflow && (
        <div className="workspace-selector">
          {workspaces && workspaces.length > 0 ? (
            <WorkspacePicker
              workspaces={workspaces}
              activeWorkspace={activeWorkspace ?? null}
              onSelect={(ws) => onSelectWorkspace?.(ws)}
            />
          ) : (
            <span className="text-sm text-[var(--text-secondary)]">
              No workspaces yet
            </span>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={onCreateWorkspace}
          >
            + New Workspace
          </Button>
          {syncStatus && (
            <span className={`sync-status sync-status-${syncStatus.status}`}>
              {syncStatus.status === "syncing" && "⟳ "}
              {syncStatus.status === "synced" && "✓ "}
              {syncStatus.status === "error" && "✗ "}
              {syncStatus.message}
            </span>
          )}
        </div>
      )}

      {(activeView === "agent-workspace" || activeView === "agent-detail" || activeView === "org-chart") && (
        <Button variant="ghost" size="sm" className="top-bar-back" onClick={onBackToFlow}>
          ← Back
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

      {editingWorkflow && flow && (
        <>
          <Button variant="ghost" size="sm" className="top-bar-back" onClick={() => onNavigate?.("workflows")}>
            ← Workflows
          </Button>
          <span className="flow-name" title={`${editingWorkflow.workspace}/${editingWorkflow.name}`}>
            {flow.name}
          </span>
        </>
      )}

      {activeView === "flow-editor" && !editingWorkflow && flow && (
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

      {(activeView === "agent-workspace" || activeView === "agent-detail") && agentName && (
        <>
          <span className="top-bar-agent-name">{agentName}</span>
          {sessionSummary && (
            <span className="top-bar-session-summary">{sessionSummary}</span>
          )}
        </>
      )}

      <div className="spacer" />

      {editingWorkflow && onRunWorkflow && (
        <Button
          size="sm"
          onClick={onRunWorkflow}
          disabled={!connected}
          title="Run this workflow"
        >
          Run
        </Button>
      )}

      {editingWorkflow && onSaveWorkflow && (
        <Button
          size="sm"
          onClick={onSaveWorkflow}
          disabled={!connected}
          title="Save workflow locally"
        >
          Save
        </Button>
      )}

      {editingWorkflow && onPublish && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onPublish}
          disabled={!connected}
          title="Save and push to GitHub"
        >
          Publish
        </Button>
      )}

      {activeView === "flow-editor" && !editingWorkflow && flow && (
        <>
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
        </>
      )}

      <div className="connection-status">
        <div
          className={`connection-dot ${connected ? "connected" : "disconnected"}`}
          title={connected ? "Connected (Tauri IPC)" : "Disconnected"}
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

/** Searchable workspace picker (combobox pattern) */
function WorkspacePicker({
  workspaces,
  activeWorkspace,
  onSelect,
}: {
  workspaces: string[];
  activeWorkspace: string | null;
  onSelect: (ws: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return workspaces;
    return workspaces.filter((ws) => ws.toLowerCase().includes(q));
  }, [workspaces, query]);

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  // Focus input when opening
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  const handleSelect = (ws: string) => {
    onSelect(ws);
    setOpen(false);
    setQuery("");
  };

  return (
    <div className="ws-picker" ref={containerRef}>
      <button
        className="ws-picker-trigger"
        onClick={() => setOpen(!open)}
        type="button"
      >
        <span className="ws-picker-trigger-text">
          {activeWorkspace || "Select workspace"}
        </span>
        <ChevronDown size={12} className={`ws-picker-chevron${open ? " ws-picker-chevron-open" : ""}`} />
      </button>
      {open && (
        <div className="ws-picker-dropdown">
          <div className="ws-picker-search">
            <Search size={12} className="ws-picker-search-icon" />
            <input
              ref={inputRef}
              className="ws-picker-search-input"
              type="text"
              placeholder="Search workspaces..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  if (query) { setQuery(""); } else { setOpen(false); }
                } else if (e.key === "Enter" && filtered.length === 1) {
                  handleSelect(filtered[0]);
                }
              }}
            />
            {query && (
              <button
                className="ws-picker-search-clear"
                onClick={() => { setQuery(""); inputRef.current?.focus(); }}
                aria-label="Clear search"
              >
                <X size={10} />
              </button>
            )}
          </div>
          <div className="ws-picker-list">
            {filtered.map((ws) => (
              <button
                key={ws}
                className={`ws-picker-item${ws === activeWorkspace ? " ws-picker-item-active" : ""}`}
                onClick={() => handleSelect(ws)}
                type="button"
              >
                <span>{ws}</span>
                {ws === activeWorkspace && <Check size={12} />}
              </button>
            ))}
            {filtered.length === 0 && (
              <div className="ws-picker-empty">
                {query ? `No workspaces matching "${query}"` : "No workspaces"}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ThemeSelector() {
  const { theme, setThemeId } = useTheme();
  const branded = themes.filter((t) => t.group === "branded");
  const presets = themes.filter((t) => t.group === "preset");

  return (
    <Select value={theme.id} onValueChange={setThemeId}>
      <SelectTrigger size="sm" className="text-xs h-8 min-w-[120px]">
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper" sideOffset={4}>
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
