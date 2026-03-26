import { useState, useEffect, useRef, useCallback, useContext } from "react";
import * as api from "../api/client";
import { log } from "../api/logger";
import { AuthContext } from "./AuthGate";
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

      {activeView === "agent-workspace" && (
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
        <>
          <span className="top-bar-agent-name">{agentName}</span>
          {sessionSummary && (
            <span className="top-bar-session-summary">{sessionSummary}</span>
          )}
        </>
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

      <ThemeSelector />

      <Button variant="ghost" size="sm" onClick={onSettingsClick}>
        Settings
      </Button>

      <ProfileMenu />
    </div>
  );
}

function VmEnvSetter({ vmId }: { vmId: number }) {
  const [webhookInput, setWebhookInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const handleSave = async () => {
    if (!webhookInput.trim()) return;
    setSaving(true);
    setSaved(false);
    try {
      const token = localStorage.getItem("cthulu_auth_token") || "";
      await fetch("/api/auth/vm-env", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Authorization": `Bearer ${token}` },
        body: JSON.stringify({ env: { SLACK_WEBHOOK_URL: webhookInput.trim() } }),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch { /* logged */ }
    setSaving(false);
  };

  return (
    <div style={{ marginTop: 6 }}>
      <div style={{ fontSize: 11, color: "var(--muted)", marginBottom: 4 }}>Slack Webhook → VM{vmId}</div>
      <div style={{ display: "flex", gap: 4 }}>
        <input
          value={webhookInput}
          onChange={(e) => setWebhookInput(e.target.value)}
          placeholder="https://hooks.slack.com/..."
          className="profile-name-input"
          style={{ flex: 1, fontSize: 11 }}
        />
        <button
          className="profile-menu-btn"
          onClick={handleSave}
          disabled={saving || !webhookInput.trim()}
        >
          {saving ? "..." : saved ? "Saved" : "Set"}
        </button>
      </div>
    </div>
  );
}

function ProfileMenu() {
  const auth = useContext(AuthContext);
  const [open, setOpen] = useState(false);
  const [profile, setProfile] = useState<api.UserProfile | null>(null);
  const [teams, setTeams] = useState<api.TeamSummary[]>([]);
  const [editingName, setEditingName] = useState(false);
  const [nameInput, setNameInput] = useState("");
  const [editingKey, setEditingKey] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    api.getProfile().then(setProfile).catch(() => {});
    api.listTeams().then((r) => setTeams(r.teams)).catch(() => {});
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleSaveName = async () => {
    if (!nameInput.trim()) return;
    try {
      const updated = await api.updateProfile({ name: nameInput.trim() });
      setProfile(updated);
      setEditingName(false);
    } catch { /* logged */ }
  };

  const handleSaveKey = async () => {
    try {
      const updated = await api.updateProfile({ anthropic_api_key: keyInput });
      setProfile(updated);
      setEditingKey(false);
      setKeyInput("");
    } catch { /* logged */ }
  };

  const handleCreateTeam = async () => {
    const name = prompt("Team name:");
    if (!name?.trim()) return;
    try {
      await api.createTeam(name.trim());
      const r = await api.listTeams();
      setTeams(r.teams);
    } catch { /* logged */ }
  };

  if (!auth) return null;

  return (
    <div className="profile-menu-container" ref={menuRef}>
      <button
        className="profile-menu-trigger"
        onClick={() => setOpen(!open)}
        title={profile?.email || "Profile"}
      >
        {profile?.name?.[0]?.toUpperCase() || profile?.email?.[0]?.toUpperCase() || "?"}
      </button>

      {open && (
        <div className="profile-menu-dropdown">
          <div className="profile-menu-header">
            {editingName ? (
              <div style={{ display: "flex", gap: 4 }}>
                <input
                  value={nameInput}
                  onChange={(e) => setNameInput(e.target.value)}
                  placeholder="Your name"
                  className="profile-name-input"
                  autoFocus
                  onKeyDown={(e) => e.key === "Enter" && handleSaveName()}
                />
                <button className="profile-menu-btn" onClick={handleSaveName}>Save</button>
              </div>
            ) : (
              <>
                <div className="profile-menu-name">
                  {profile?.name || profile?.email || "..."}
                </div>
                <div className="profile-menu-email">{profile?.email}</div>
                <button
                  className="profile-menu-btn"
                  onClick={() => { setNameInput(profile?.name || ""); setEditingName(true); }}
                >
                  Edit name
                </button>
              </>
            )}
          </div>

          {/* VM Status + Env Vars */}
          <div className="profile-menu-section">
            <div className="profile-menu-section-header"><span>VM</span></div>
            {profile?.vm_id != null ? (
              <>
                <div style={{ fontSize: 12, color: "var(--success)", fontWeight: 600, marginBottom: 8 }}>
                  VM{profile.vm_id} connected — running Claude Code
                </div>
                <VmEnvSetter vmId={profile.vm_id} />
              </>
            ) : (
              <span style={{ fontSize: 12, color: "var(--muted)" }}>No VM assigned</span>
            )}
          </div>

          <div className="profile-menu-section">
            <div className="profile-menu-section-header">
              <span>API Key</span>
            </div>
            {editingKey ? (
              <div style={{ display: "flex", gap: 4 }}>
                <input
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                  placeholder="sk-ant-..."
                  type="password"
                  className="profile-name-input"
                  autoFocus
                  onKeyDown={(e) => e.key === "Enter" && handleSaveKey()}
                />
                <button className="profile-menu-btn" onClick={handleSaveKey}>Save</button>
                <button className="profile-menu-btn" onClick={() => setEditingKey(false)}>Cancel</button>
              </div>
            ) : (
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <span style={{ fontSize: 12, color: profile?.has_api_key ? "var(--success)" : "var(--muted)" }}>
                  {profile?.has_api_key ? "Key set" : "No personal key (using server key)"}
                </span>
                <button className="profile-menu-btn" onClick={() => setEditingKey(true)}>
                  {profile?.has_api_key ? "Change" : "Set key"}
                </button>
              </div>
            )}
          </div>

          <div className="profile-menu-section">
            <div className="profile-menu-section-header">
              <span>Teams</span>
              <button className="profile-menu-btn" onClick={handleCreateTeam}>+ New</button>
            </div>
            {teams.length === 0 ? (
              <div className="profile-menu-empty">No teams yet</div>
            ) : (
              teams.map((t) => (
                <div key={t.id} className="profile-menu-team">
                  {t.name} <span style={{ opacity: 0.5, fontSize: 11 }}>({t.member_count})</span>
                </div>
              ))
            )}
          </div>

          <button className="profile-menu-logout" onClick={auth.logout}>
            Log out
          </button>
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
