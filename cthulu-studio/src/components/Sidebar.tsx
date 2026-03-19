import { useState, useEffect, useCallback } from "react";
import { STUDIO_ASSISTANT_ID, type FlowSummary, type Flow, type NodeTypeSchema, type AgentSummary, type SavedPrompt, type ActiveView } from "../types/flow";
import { listAgents, createAgent, deleteAgent, listPrompts, savePrompt, deletePrompt as deletePromptApi, listAgentSessions, newAgentSession, listTeams } from "../api/client";
import type { InteractSessionInfo } from "../api/client";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import TemplateGallery from "./TemplateGallery";
import LooneyTunesShow from "./LooneyTunesShow";

interface SidebarProps {
  // Flow list
  flows: FlowSummary[];
  activeFlowId: string | null;
  onSelectFlow: (id: string) => void;
  onCreateFlow: () => void;
  onImportTemplate: (flow: Flow) => void;
  onToggleEnabled: (flowId: string) => void;
  // Agent + session selection
  selectedAgentId: string | null;
  selectedSessionId: string | null;
  onSelectSession: (agentId: string, sessionId: string) => void;
  agentListKey: number;
  onAgentCreated: (id: string) => void;
  // Prompts
  selectedPromptId: string | null;
  onSelectPrompt: (id: string) => void;
  promptListKey: number;
  // Node palette (only in flow editor view)
  activeView: ActiveView;
  nodeTypes: NodeTypeSchema[];
  onGrab: (nodeType: NodeTypeSchema) => void;
  onCollapse: () => void;
  onSelectDashboard?: () => void;
}

const typeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Sidebar({
  flows,
  activeFlowId,
  onSelectFlow,
  onCreateFlow,
  onImportTemplate,
  onToggleEnabled,
  selectedAgentId,
  selectedSessionId,
  onSelectSession,
  agentListKey,
  onAgentCreated,
  selectedPromptId,
  onSelectPrompt,
  promptListKey,
  activeView,
  nodeTypes,
  onGrab,
  onCollapse,
  onSelectDashboard,
}: SidebarProps) {
  const [showGallery, setShowGallery] = useState(false);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [agentMeta, setAgentMeta] = useState<Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>>(new Map());
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set());
  const [userTeams, setUserTeams] = useState<{ id: string; name: string }[]>([]);

  // Load user's teams for creating team agents
  useEffect(() => {
    listTeams().then((res) => setUserTeams(res.teams.map((t) => ({ id: t.id, name: t.name })))).catch(() => {});
  }, []);

  const refreshAgents = useCallback(async () => {
    try {
      const list = await listAgents();
      setAgents(list.filter((a) => !a.subagent_only));
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshAgents();
  }, [refreshAgents, agentListKey]);

  // Poll agent session data for tree display
  useEffect(() => {
    if (agents.length === 0) return;

    const fetchMeta = async () => {
      const results = await Promise.allSettled(
        agents.map((a) => listAgentSessions(a.id).then((info) => ({ id: a.id, info })))
      );
      const next = new Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>();
      for (const r of results) {
        if (r.status === "fulfilled") {
          const { id, info } = r.value;
          const busy = info.sessions.some((s) => s.busy);
          const cost = info.sessions.reduce((sum, s) => sum + s.total_cost, 0);
          next.set(id, { busy, sessions: info.sessions, cost });
        }
      }
      setAgentMeta(next);
    };

    fetchMeta();
    const interval = setInterval(fetchMeta, 5000);
    return () => clearInterval(interval);
  }, [agents]);

  const refreshPrompts = useCallback(async () => {
    try {
      setPrompts(await listPrompts());
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshPrompts();
  }, [refreshPrompts, promptListKey]);

  async function handleCreatePrompt() {
    try {
      const { id } = await savePrompt({
        title: "New Prompt",
        summary: "",
        source_flow_name: "",
        tags: [],
      });
      await refreshPrompts();
      onSelectPrompt(id);
    } catch (e) {
      console.error("Failed to create prompt:", e);
    }
  }

  async function handleDeletePrompt(e: React.MouseEvent, id: string) {
    e.stopPropagation();
    if (!confirm("Delete this prompt?")) return;
    try {
      await deletePromptApi(id);
      await refreshPrompts();
    } catch (e) {
      console.error("Failed to delete prompt:", e);
    }
  }

  async function handleCreateAgent() {
    try {
      const { id } = await createAgent({ name: "New Agent" });
      await refreshAgents();
      onAgentCreated(id);
    } catch (e) {
      console.error("Failed to create agent:", e);
    }
  }

  async function handleCreateTeamAgent(teamId: string) {
    try {
      const team = userTeams.find((t) => t.id === teamId);
      const { id } = await createAgent({
        name: `${team?.name || "Team"} Agent`,
        team_id: teamId,
      });
      await refreshAgents();
      onAgentCreated(id);
    } catch (e) {
      console.error("Failed to create team agent:", e);
    }
  }

  async function handleDeleteAgent(e: React.MouseEvent, agentId: string) {
    e.stopPropagation();
    if (!confirm("Delete this agent?")) return;
    try {
      await deleteAgent(agentId);
      await refreshAgents();
    } catch (err) {
      console.error("Failed to delete agent:", err);
    }
  }

  function handleNewFlowClick() {
    setShowGallery(true);
  }

  function handleGalleryImport(flow: Flow) {
    setShowGallery(false);
    onImportTemplate(flow);
  }

  function handleBlank() {
    setShowGallery(false);
    onCreateFlow();
  }

  const grouped = {
    trigger: nodeTypes.filter((n) => n.node_type === "trigger"),
    source: nodeTypes.filter((n) => n.node_type === "source"),
    executor: nodeTypes.filter((n) => n.node_type === "executor"),
    sink: nodeTypes.filter((n) => n.node_type === "sink"),
  };

  return (
    <div className="unified-sidebar">
      <div className="sidebar-collapse-bar">
        <button className="sidebar-collapse-btn" onClick={onCollapse} title="Collapse sidebar">
          ◨
        </button>
      </div>
      {showGallery && (
        <TemplateGallery
          onImport={handleGalleryImport}
          onBlank={handleBlank}
          onClose={() => setShowGallery(false)}
        />
      )}

      <LooneyTunesShow />

      {/* Dashboard nav item */}
      <div
        className={`sidebar-item sidebar-dashboard-item${activeView === "dashboard" ? " active" : ""}`}
        onClick={() => onSelectDashboard?.()}
      >
        <div className="sidebar-item-row">
          <span className="sidebar-item-name">Dashboard</span>
        </div>
      </div>

      {/* Agents section (primary, expanded by default) */}
      <Collapsible defaultOpen className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Agents</h2>
            <div style={{ flex: 1 }} />
            <select
              className="sidebar-agent-create-select"
              value=""
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => {
                const val = e.target.value;
                if (val === "personal") handleCreateAgent();
                else if (val) handleCreateTeamAgent(val);
                e.target.value = "";
              }}
            >
              <option value="">+</option>
              <option value="personal">Personal Agent</option>
              {userTeams.length > 0 && <option disabled>── Teams ──</option>}
              {userTeams.map((t) => (
                <option key={t.id} value={t.id}>{t.name} Agent</option>
              ))}
            </select>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {[...agents].filter((a) => !a.team_id).sort((a, b) => {
              if (a.id === STUDIO_ASSISTANT_ID) return -1;
              if (b.id === STUDIO_ASSISTANT_ID) return 1;
              return 0;
            }).map((agent) => {
              const meta = agentMeta.get(agent.id);
              const isExpanded = expandedAgents.has(agent.id);
              const isActive = agent.id === selectedAgentId && activeView === "agent-workspace";
              const sessions = meta?.sessions ?? [];

              return (
                <div key={agent.id} className="sb-agent">
                  <div
                    className={`sb-agent-row${isActive ? " sb-agent-active" : ""}`}
                    onClick={() => {
                      setExpandedAgents((prev) => {
                        const next = new Set(prev);
                        if (next.has(agent.id)) next.delete(agent.id);
                        else next.add(agent.id);
                        return next;
                      });
                      if (sessions.length > 0) {
                        onSelectSession(agent.id, sessions[0].session_id);
                      }
                    }}
                  >
                    <span className="sb-agent-chevron">{isExpanded ? "▾" : "▸"}</span>
                    {meta?.busy && <span className="sb-agent-pulse" />}
                    <span className="sb-agent-name">{agent.name}</span>
                    {meta && meta.cost > 0 && (
                      <span className="sb-agent-cost">${meta.cost.toFixed(2)}</span>
                    )}
                    {agent.id !== STUDIO_ASSISTANT_ID && (
                      <button
                        className="ghost sb-agent-delete"
                        onClick={(e) => handleDeleteAgent(e, agent.id)}
                        title="Delete agent"
                      >
                        ×
                      </button>
                    )}
                  </div>
                  {isExpanded && (
                    <div className="sb-sessions">
                      {sessions.map((s) => {
                        const isSessionActive = s.session_id === selectedSessionId && agent.id === selectedAgentId;
                        const label = s.summary || (s.kind === "flow_run" ? `Run: ${s.flow_run?.flow_name ?? ""}` : "New session");
                        return (
                          <div
                            key={s.session_id}
                            className={`sb-session${isSessionActive ? " sb-session-active" : ""}`}
                            onClick={() => onSelectSession(agent.id, s.session_id)}
                          >
                            {s.busy && <span className="sb-session-pulse" />}
                            <span className="sb-session-label">{label}</span>
                            {s.total_cost > 0 && (
                              <span className="sb-session-cost">${s.total_cost.toFixed(2)}</span>
                            )}
                          </div>
                        );
                      })}
                      <button
                        className="sb-session-new"
                        onClick={async (e) => {
                          e.stopPropagation();
                          try {
                            const result = await newAgentSession(agent.id);
                            onSelectSession(agent.id, result.session_id);
                          } catch (err) {
                            console.error("Failed to create session:", err);
                          }
                        }}
                      >
                        + New Session
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {agents.length === 0 && (
              <div className="sidebar-item-empty">No agents yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Team Agents — one sub-section per team */}
      {userTeams.map((team) => {
        const teamAgents = agents.filter((a) => a.team_id === team.id);
        return (
          <Collapsible key={team.id} defaultOpen className="sidebar-section">
            <CollapsibleTrigger asChild>
              <div className="sidebar-section-header">
                <span className="sidebar-chevron">▶</span>
                <h2 className="sidebar-team-name">
                  <span className="sidebar-team-badge" />
                  {team.name}
                </h2>
                <div style={{ flex: 1 }} />
                <button
                  className="ghost sidebar-action-btn"
                  onClick={(e) => { e.stopPropagation(); handleCreateTeamAgent(team.id); }}
                >
                  +
                </button>
              </div>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="sidebar-section-body">
                {teamAgents.length === 0 && (
                  <div className="sidebar-empty">No agents yet</div>
                )}
                {teamAgents.map((agent) => {
                  const meta = agentMeta.get(agent.id);
                  const isActive = agent.id === selectedAgentId && activeView === "agent-workspace";
                  return (
                    <div key={agent.id} className="sb-agent">
                      <div
                        className={`sb-agent-row sb-agent-team${isActive ? " sb-agent-active" : ""}`}
                        onClick={() => {
                          const sessions = meta?.sessions ?? [];
                          if (sessions.length > 0) onSelectSession(agent.id, sessions[0].session_id);
                        }}
                      >
                        <span className="sb-agent-name">{agent.name}</span>
                        {meta?.busy && <span className="sb-agent-dot sb-agent-busy" />}
                        <span className="sb-agent-team-tag">team</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </CollapsibleContent>
          </Collapsible>
        );
      })}

      {/* Flows section (collapsed by default) */}
      <Collapsible className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Flows</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleNewFlowClick();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {flows.map((flow) => (
              <div
                key={flow.id}
                className={`sidebar-item${flow.id === activeFlowId && activeView === "flow-editor" ? " active" : ""}${!flow.enabled ? " disabled" : ""}`}
                onClick={() => onSelectFlow(flow.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">{flow.name}</div>
                  <Switch
                    checked={flow.enabled}
                    onCheckedChange={() => onToggleEnabled(flow.id)}
                    onClick={(e) => e.stopPropagation()}
                    className="data-[state=checked]:bg-[var(--success)]"
                  />
                </div>
                <div className="sidebar-item-meta">{flow.node_count} nodes</div>
              </div>
            ))}
            {flows.length === 0 && (
              <div className="sidebar-item-empty">No flows yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Prompts section */}
      <Collapsible defaultOpen className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Prompts</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleCreatePrompt();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {prompts.map((p) => (
              <div
                key={p.id}
                className={`sidebar-item${p.id === selectedPromptId && activeView === "prompt-editor" ? " active" : ""}`}
                onClick={() => onSelectPrompt(p.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">{p.title}</div>
                  <button
                    className="ghost sidebar-delete-btn"
                    onClick={(e) => handleDeletePrompt(e, p.id)}
                    title="Delete prompt"
                  >
                    ×
                  </button>
                </div>
                {p.tags.length > 0 && (
                  <div className="sidebar-item-meta">{p.tags.join(", ")}</div>
                )}
              </div>
            ))}
            {prompts.length === 0 && (
              <div className="sidebar-item-empty">No prompts yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Node palette — only visible in flow editor with an active flow */}
      {activeView === "flow-editor" && activeFlowId && (
        <Collapsible defaultOpen className="sidebar-section sidebar-palette-section">
          <CollapsibleTrigger asChild>
            <div className="sidebar-section-header">
              <span className="sidebar-chevron">▶</span>
              <h2>Nodes</h2>
            </div>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="sidebar-section-body">
              {(["trigger", "source", "executor", "sink"] as const).map((type) => (
                <div key={type}>
                  {grouped[type].map((nt) => (
                    <div
                      key={nt.kind}
                      className="palette-item"
                      onMouseDown={(e) => {
                        e.preventDefault();
                        onGrab(nt);
                      }}
                    >
                      <div
                        className="palette-dot"
                        style={{ background: typeColors[nt.node_type] }}
                      />
                      {nt.label}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}

    </div>
  );
}
