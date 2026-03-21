import { useState, useEffect, useRef, useCallback } from "react";
import { useAuth } from "./AuthGate";
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

      <ProfileMenu />
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

function ProfileMenu() {
  const { logout, userEmail } = useAuth();
  const [open, setOpen] = useState(false);
  const [profile, setProfile] = useState<{ name: string | null; email: string } | null>(null);
  const [teams, setTeams] = useState<{ id: string; name: string }[]>([]);
  const [editingName, setEditingName] = useState(false);
  const [nameInput, setNameInput] = useState("");
  const [newTeamName, setNewTeamName] = useState("");
  const [expandedTeam, setExpandedTeam] = useState<string | null>(null);
  const [teamMembers, setTeamMembers] = useState<Record<string, { id: string; email: string; name: string | null }[]>>({});
  const [memberError, setMemberError] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);

  if (!userEmail) return null;

  const loadData = useCallback(async () => {
    try {
      const [p, t] = await Promise.all([api.getProfile(), api.listTeams()]);
      setProfile({ name: p.name, email: p.email });
      setTeams(t.teams.map((tm) => ({ id: tm.id, name: tm.name })));
    } catch { /* ignore if not authed */ }
  }, []);

  useEffect(() => {
    if (open) loadData();
  }, [open, loadData]);

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
    await api.updateProfile({ name: nameInput.trim() });
    setProfile((p) => p ? { ...p, name: nameInput.trim() } : p);
    setEditingName(false);
  };

  const loadTeamMembers = async (teamId: string) => {
    const { team } = await api.getTeam(teamId);
    setTeamMembers((prev) => ({ ...prev, [teamId]: team.members as { id: string; email: string; name: string | null }[] }));
  };

  const handleExpandTeam = async (teamId: string) => {
    if (expandedTeam === teamId) { setExpandedTeam(null); return; }
    setExpandedTeam(teamId);
    setMemberError("");
    try { await loadTeamMembers(teamId); } catch { /* ignore */ }
  };

  const handleRemoveMember = async (teamId: string, userId: string) => {
    try {
      await api.removeTeamMember(teamId, userId);
      await loadTeamMembers(teamId);
    } catch { /* ignore */ }
  };

  const handleCreateTeam = async () => {
    if (!newTeamName.trim()) return;
    const { team } = await api.createTeam(newTeamName.trim());
    setTeams((prev) => [...prev, { id: team.id, name: team.name }]);
    setNewTeamName("");
  };

  return (
    <div className="profile-menu" ref={menuRef}>
      <Button variant="ghost" size="sm" onClick={() => setOpen(!open)}>
        {profile?.name || userEmail.split("@")[0]}
      </Button>

      {open && (
        <div className="profile-dropdown">
          <div className="profile-section">
            <div className="profile-label">Profile</div>
            <div className="profile-email">{profile?.email || userEmail}</div>
            {editingName ? (
              <div className="profile-name-edit">
                <input className="auth-input" value={nameInput} onChange={(e) => setNameInput(e.target.value)}
                  placeholder="Display name" autoFocus onKeyDown={(e) => e.key === "Enter" && handleSaveName()} />
                <Button variant="ghost" size="sm" onClick={handleSaveName}>Save</Button>
              </div>
            ) : (
              <button className="profile-name-btn"
                onClick={() => { setNameInput(profile?.name || ""); setEditingName(true); }}>
                {profile?.name || "Set display name"}
              </button>
            )}
          </div>

          <div className="profile-divider" />

          <div className="profile-section">
            <div className="profile-label">Teams</div>
            {teams.length === 0 && <div className="profile-empty">No teams yet</div>}
            {teams.map((t) => (
              <div key={t.id} className="profile-team-card">
                <button className="profile-team-header" onClick={() => handleExpandTeam(t.id)}>
                  <span>{t.name}</span>
                  <span className="profile-team-chevron">{expandedTeam === t.id ? "▾" : "▸"}</span>
                </button>
                {expandedTeam === t.id && (
                  <div className="profile-team-body">
                    {(teamMembers[t.id] || []).map((m) => (
                      <div key={m.id} className="profile-member">
                        <span className="profile-member-info">
                          <span className="profile-member-name">{m.name || m.email.split("@")[0]}</span>
                          <span className="profile-member-email">{m.email}</span>
                        </span>
                        <button className="profile-member-remove" onClick={() => handleRemoveMember(t.id, m.id)} title="Remove member">x</button>
                      </div>
                    ))}
                    <UserSearchInput onSelect={async (_userId, email) => {
                      try {
                        await api.addTeamMember(t.id, email);
                        await loadTeamMembers(t.id);
                      } catch (e) { setMemberError(e instanceof Error ? e.message : "Failed"); }
                    }} />
                    {memberError && <div className="auth-error" style={{ padding: "2px 0" }}>{memberError}</div>}
                  </div>
                )}
              </div>
            ))}
            <div className="profile-name-edit">
              <input className="auth-input" value={newTeamName} onChange={(e) => setNewTeamName(e.target.value)}
                placeholder="New team name" onKeyDown={(e) => e.key === "Enter" && handleCreateTeam()} />
              <Button variant="ghost" size="sm" onClick={handleCreateTeam}>Create</Button>
            </div>
          </div>

          <div className="profile-divider" />
          <button className="profile-signout" onClick={logout}>Sign Out</button>
        </div>
      )}
    </div>
  );
}

function UserSearchInput({ onSelect }: { onSelect: (userId: string, email: string) => void }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<api.UserSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleChange = (value: string) => {
    setQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (value.trim().length < 2) { setResults([]); return; }

    setSearching(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const { users } = await api.searchUsers(value.trim());
        setResults(users);
      } catch { setResults([]); }
      finally { setSearching(false); }
    }, 300);
  };

  return (
    <div className="user-search">
      <input className="auth-input" placeholder="Add member (search name or email)"
        value={query} onChange={(e) => handleChange(e.target.value)} />
      {(results.length > 0 || searching) && (
        <div className="user-search-results">
          {searching && <div className="user-search-item user-search-loading">Searching...</div>}
          {results.map((u) => (
            <button key={u.id} className="user-search-item"
              onClick={() => { onSelect(u.id, u.email); setQuery(""); setResults([]); }}>
              <span className="user-search-name">{u.name || u.email.split("@")[0]}</span>
              <span className="user-search-email">{u.email}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
