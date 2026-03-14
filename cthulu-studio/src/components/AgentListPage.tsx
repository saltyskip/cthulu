import { useState, useEffect, useMemo, useCallback } from "react";
import * as api from "../api/client";
import type { AgentSummary, HeartbeatRun, ProjectMeta } from "../types/flow";
import { EntityRow } from "./EntityRow";
import { StatusBadge } from "./StatusBadge";
import { deriveAgentStatus, agentStatusDot, agentStatusDotDefault } from "../lib/status-colors";
import { STUDIO_ASSISTANT_ID } from "../types/flow";
import { List, Zap, Pause, AlertTriangle, FolderPlus, Plus, Building2, Network } from "lucide-react";
import { useOrg } from "../contexts/OrgContext";
import { useNavigation } from "../contexts/NavigationContext";
import { NewProjectDialog } from "./NewProjectDialog";
import { NewOrgDialog } from "./NewOrgDialog";

type FilterTab = "all" | "active" | "paused" | "error";

interface AgentListPageProps {
  onSelectAgent: (agentId: string) => void;
  onCreateAgent: () => void;
  refreshKey: number;
}

export function AgentListPage({ onSelectAgent, onCreateAgent, refreshKey }: AgentListPageProps) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [tab, setTab] = useState<FilterTab>("all");
  const [sessionMeta, setSessionMeta] = useState<Map<string, { busy: boolean; cost: number }>>(new Map());
  const [lastRuns, setLastRuns] = useState<Map<string, HeartbeatRun>>(new Map());
  const [projects, setProjects] = useState<ProjectMeta[]>([]);
  const [showNewProject, setShowNewProject] = useState(false);
  const [showNewOrg, setShowNewOrg] = useState(false);
  const { selectedOrg, selectedOrgSlug, orgs } = useOrg();
  const { setActiveView } = useNavigation();

  // Load agents
  useEffect(() => {
    api.listAgents().then(list => {
      setAgents(list.filter((a: AgentSummary) => !a.subagent_only));
    }).catch(() => {});
  }, [refreshKey]);

  // Load projects for selected org
  useEffect(() => {
    if (!selectedOrgSlug) {
      setProjects([]);
      return;
    }
    api.listAgentProjects(selectedOrgSlug).then(setProjects).catch(() => setProjects([]));
  }, [selectedOrgSlug, refreshKey]);

  const handleProjectCreated = useCallback(() => {
    setShowNewProject(false);
    if (selectedOrgSlug) {
      api.listAgentProjects(selectedOrgSlug).then(setProjects).catch(() => {});
    }
  }, [selectedOrgSlug]);

  // Scope agents to selected org's projects (unassigned agents always shown)
  const orgScopedAgents = useMemo(() => {
    if (!selectedOrgSlug || projects.length === 0) return agents;
    const projectSlugs = new Set(projects.map(p => p.slug));
    return agents.filter(a => !a.project || projectSlugs.has(a.project));
  }, [agents, projects, selectedOrgSlug]);

  // Poll session status every 5s (only for org-scoped agents)
  useEffect(() => {
    if (orgScopedAgents.length === 0) return;
    let cancelled = false;
    const poll = async () => {
      const meta = new Map<string, { busy: boolean; cost: number }>();
      for (const agent of orgScopedAgents) {
        try {
          const sessions = await api.listAgentSessions(agent.id);
          const sessionList = sessions.sessions ?? [];
          const busy = sessionList.some((s: any) => s.busy);
          const cost = sessionList.reduce((sum: number, s: any) => sum + (s.total_cost ?? 0), 0);
          meta.set(agent.id, { busy, cost });
        } catch { /* ignore */ }
      }
      if (!cancelled) setSessionMeta(meta);
    };
    poll();
    const iv = setInterval(poll, 5000);
    return () => { cancelled = true; clearInterval(iv); };
  }, [orgScopedAgents]);

  // Load last heartbeat run per agent (only for org-scoped agents)
  useEffect(() => {
    if (orgScopedAgents.length === 0) return;
    let cancelled = false;
    const loadRuns = async () => {
      const runs = new Map<string, HeartbeatRun>();
      for (const agent of orgScopedAgents) {
        try {
          const list = await api.listHeartbeatRuns(agent.id);
          if (list.length > 0) runs.set(agent.id, list[0]);
        } catch { /* ignore */ }
      }
      if (!cancelled) setLastRuns(runs);
    };
    loadRuns();
    return () => { cancelled = true; };
  }, [orgScopedAgents]);

  // Filter by status tab
  const filtered = useMemo(() => {
    return orgScopedAgents.filter(a => {
      if (tab === "all") return true;
      const meta = sessionMeta.get(a.id);
      const lastRun = lastRuns.get(a.id);
      const status = deriveAgentStatus(
        true, // We don't have heartbeat_enabled on summary; default to true
        meta?.busy ?? false,
        lastRun?.status === "failed" || lastRun?.status === "timed_out",
      );
      if (tab === "active") return status === "active" || status === "busy";
      if (tab === "paused") return status === "paused";
      if (tab === "error") return status === "error";
      return true;
    });
  }, [orgScopedAgents, tab, sessionMeta, lastRuns]);

  // Sort: studio-assistant first, then alphabetical
  const sorted = useMemo(() => {
    return [...filtered].sort((a, b) => {
      if (a.id === STUDIO_ASSISTANT_ID) return -1;
      if (b.id === STUDIO_ASSISTANT_ID) return 1;
      return a.name.localeCompare(b.name);
    });
  }, [filtered]);

  // Show landing page when no org is set up yet
  if (orgs.length === 0) {
    return (
      <div className="agent-list-page">
        <div className="agent-list-landing">
          <div className="agent-list-landing-icon">
            <Building2 size={48} strokeWidth={1} />
          </div>
          <h2 className="agent-list-landing-title">Get Started</h2>
          <p className="agent-list-landing-desc">
            Create an organization to start managing your agents and projects.
            Your data is stored in a private <code>cthulu-agents</code> GitHub repo.
          </p>
          <button className="agent-list-landing-btn" onClick={() => setShowNewOrg(true)}>
            <Building2 size={16} />
            Create Organization
          </button>
        </div>
        {showNewOrg && <NewOrgDialog onClose={() => setShowNewOrg(false)} />}
      </div>
    );
  }

  return (
    <div className="agent-list-page">
      <div className="agent-list-header">
        <div className="agent-list-header-left">
          <h2 className="agent-list-title">
            {selectedOrg ? selectedOrg.name : "Agents"}
          </h2>
          {selectedOrg && (
            <span className="agent-list-org-badge">{projects.length} project{projects.length !== 1 ? "s" : ""}</span>
          )}
        </div>
        <div className="agent-list-actions">
          <button className="agent-list-action-btn" onClick={() => setActiveView("org-chart")} title="Org Chart">
            <Network size={14} />
            Org Chart
          </button>
          <button className="agent-list-action-btn" onClick={() => setShowNewProject(true)} title="New Project">
            <FolderPlus size={14} />
            New Project
          </button>
          <button className="agent-list-new-btn" onClick={onCreateAgent}>
            <Plus size={14} />
            New Agent
          </button>
        </div>
      </div>

      <div className="agent-list-tabs">
        {([
          { id: "all" as FilterTab, label: "All", icon: List },
          { id: "active" as FilterTab, label: "Active", icon: Zap },
          { id: "paused" as FilterTab, label: "Paused", icon: Pause },
          { id: "error" as FilterTab, label: "Error", icon: AlertTriangle },
        ]).map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            className={`agent-list-tab${tab === id ? " agent-list-tab-active" : ""}`}
            onClick={() => setTab(id)}
          >
            <Icon className="agent-list-tab-icon" />
            {label}
          </button>
        ))}
      </div>

      <div className="agent-list-container">
        {sorted.length === 0 ? (
          <div className="agent-list-empty">
            <div className="agent-list-empty-content">
              {tab === "all" ? (
                <>
                  <p className="agent-list-empty-title">No agents yet</p>
                  <p className="agent-list-empty-desc">
                    {projects.length === 0
                      ? "Start by creating a project, then add agents to it."
                      : "Create an agent and assign it to a project."}
                  </p>
                  <div className="agent-list-empty-actions">
                    {projects.length === 0 && (
                      <button className="agent-list-empty-btn" onClick={() => setShowNewProject(true)}>
                        <FolderPlus size={14} />
                        Create Project
                      </button>
                    )}
                    <button className="agent-list-empty-btn agent-list-empty-btn-primary" onClick={onCreateAgent}>
                      <Plus size={14} />
                      Create Agent
                    </button>
                  </div>
                </>
              ) : (
                <p className="agent-list-empty-title">No {tab} agents</p>
              )}
            </div>
          </div>
        ) : (
          sorted.map(agent => {
            const meta = sessionMeta.get(agent.id);
            const lastRun = lastRuns.get(agent.id);
            const status = deriveAgentStatus(
              true,
              meta?.busy ?? false,
              lastRun?.status === "failed" || lastRun?.status === "timed_out",
            );
            const dotColor = agentStatusDot[status] ?? agentStatusDotDefault;

            return (
              <EntityRow
                key={agent.id}
                onClick={() => onSelectAgent(agent.id)}
                leading={
                  <div className="sb-agent-status-dot" style={{ background: dotColor }} />
                }
                title={agent.name}
                subtitle={agent.description || undefined}
                trailing={
                  <>
                    {meta?.busy && (
                      <span className="live-indicator">
                        <span className="live-indicator-dot" />
                        Live
                      </span>
                    )}
                    <StatusBadge status={status} />
                    {meta && meta.cost > 0 && (
                      <span className="agent-cost-label">${meta.cost.toFixed(2)}</span>
                    )}
                  </>
                }
              />
            );
          })
        )}
      </div>

      {showNewProject && (
        <NewProjectDialog
          onClose={() => setShowNewProject(false)}
          onCreated={handleProjectCreated}
        />
      )}
      {showNewOrg && <NewOrgDialog onClose={() => setShowNewOrg(false)} />}
    </div>
  );
}
