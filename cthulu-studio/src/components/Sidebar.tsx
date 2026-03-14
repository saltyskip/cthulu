import { useState, useEffect, useCallback, useMemo, useDeferredValue, useRef } from "react";
import { STUDIO_ASSISTANT_ID, type FlowSummary, type Flow, type NodeTypeSchema, type AgentSummary, type SavedPrompt, type ActiveView, type WorkflowSummary } from "../types/flow";
import { listAgents, deleteAgent, listPrompts, savePrompt, deletePrompt as deletePromptApi, listAgentSessions, syncAgentRepo } from "../api/client";
import type { InteractSessionInfo } from "../api/client";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import { PanelLeftClose, ChevronRight, Plus, X, Play, Search } from "lucide-react";
import ConfirmDialog, { useConfirm } from "./ConfirmDialog";
import { useWorkflowContext } from "../contexts/WorkflowContext";
import { agentStatusDot, agentStatusDotDefault, deriveAgentStatus } from "../lib/status-colors";
import { SidebarProjects } from "./SidebarProjects";
import { NewAgentDialog } from "./NewAgentDialog";

interface SidebarProps {
  // Agent selection (new Paperclip-style)
  selectedAgentId: string | null;
  onSelectAgent: (agentId: string) => void;
  agentListKey: number;
  onAgentCreated: (id: string) => void;
  // Prompts
  selectedPromptId: string | null;
  onSelectPrompt: (id: string) => void;
  promptListKey: number;
  // View state
  activeView: ActiveView;
  onCollapse: () => void;
  // Node palette (only in flow editor view)
  activeFlowId: string | null;
  nodeTypes: NodeTypeSchema[];
  onGrab: (nodeType: NodeTypeSchema) => void;
  // Workflows sidebar (only in workflows view)
  activeWorkspace: string | null;
  workflows: WorkflowSummary[];
  onSelectWorkflow: (workspace: string, name: string) => void;
  onCreateWorkflow: () => void;
  onDeleteWorkflow: (workspace: string, name: string) => void;
  editingWorkflow?: { workspace: string; name: string } | null;
  onToggleWorkflowEnabled?: (workspace: string, name: string) => void;
  onRunWorkflow?: (workspace: string, name: string) => void;
  isWorkflowEnabled?: (workspace: string, name: string) => boolean;
}

const typeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Sidebar({
  selectedAgentId,
  onSelectAgent,
  agentListKey,
  onAgentCreated,
  selectedPromptId,
  onSelectPrompt,
  promptListKey,
  activeView,
  onCollapse,
  activeFlowId,
  nodeTypes,
  onGrab,
  activeWorkspace,
  workflows,
  onSelectWorkflow,
  onCreateWorkflow,
  onDeleteWorkflow,
  editingWorkflow,
  onToggleWorkflowEnabled,
  onRunWorkflow,
  isWorkflowEnabled,
}: SidebarProps) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [agentMeta, setAgentMeta] = useState<Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>>(new Map());
  const [syncing, setSyncing] = useState(false);
  const [confirmState, requestConfirm] = useConfirm();

  // Shared workflow search from context
  const { workflowSearch, setWorkflowSearch } = useWorkflowContext();
  const deferredSearch = useDeferredValue(workflowSearch);
  const sidebarSearchRef = useRef<HTMLInputElement>(null);

  const filteredWorkflows = useMemo(() => {
    const q = deferredSearch.trim().toLowerCase();
    if (!q) return workflows;
    return workflows.filter((wf) =>
      wf.name.toLowerCase().includes(q) ||
      (wf.description && wf.description.toLowerCase().includes(q))
    );
  }, [workflows, deferredSearch]);

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

  const handleSyncAgents = useCallback(async () => {
    setSyncing(true);
    try {
      await syncAgentRepo();
      await refreshAgents();
    } catch (e) {
      console.error("Failed to sync agents:", e);
    } finally {
      setSyncing(false);
    }
  }, [refreshAgents]);

  // Poll agent session data for live indicators
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
    const ok = await requestConfirm("Delete prompt?", "This action cannot be undone.");
    if (!ok) return;
    try {
      await deletePromptApi(id);
      await refreshPrompts();
    } catch (e) {
      console.error("Failed to delete prompt:", e);
    }
  }

  const [showNewAgent, setShowNewAgent] = useState(false);

  function handleCreateAgent() {
    setShowNewAgent(true);
  }

  async function handleAgentDialogCreated(id: string) {
    setShowNewAgent(false);
    await refreshAgents();
    onAgentCreated(id);
  }

  async function handleDeleteAgent(e: React.MouseEvent, agentId: string) {
    e.stopPropagation();
    const ok = await requestConfirm("Delete agent?", "All sessions and data for this agent will be removed.");
    if (!ok) return;
    try {
      await deleteAgent(agentId);
      await refreshAgents();
    } catch (err) {
      console.error("Failed to delete agent:", err);
    }
  }

  const grouped = {
    trigger: nodeTypes.filter((n) => n.node_type === "trigger"),
    source: nodeTypes.filter((n) => n.node_type === "source"),
    executor: nodeTypes.filter((n) => n.node_type === "executor"),
    sink: nodeTypes.filter((n) => n.node_type === "sink"),
  };

  // Sort agents: studio-assistant first, then alphabetical
  const sortedAgents = useMemo(() =>
    [...agents].sort((a, b) => {
      if (a.id === STUDIO_ASSISTANT_ID) return -1;
      if (b.id === STUDIO_ASSISTANT_ID) return 1;
      return a.name.localeCompare(b.name);
    }),
  [agents]);

  // Unassigned agents (not under any project in the sidebar Projects section)
  const unassignedAgents = useMemo(() =>
    sortedAgents.filter(a => !a.project),
  [sortedAgents]);

  return (
    <div className="unified-sidebar">
      <div className="sidebar-collapse-bar">
        <button className="sidebar-collapse-btn" onClick={onCollapse} title="Collapse sidebar" aria-label="Collapse sidebar">
          <PanelLeftClose size={14} />
        </button>
      </div>

      {activeView === "workflows" ? (
        <>
          {/* Workflows in active workspace */}
          {activeWorkspace && (
            <Collapsible defaultOpen className="sidebar-section">
              <CollapsibleTrigger asChild>
                <div className="sidebar-section-header">
                  <span className="sidebar-chevron"><ChevronRight size={12} /></span>
                  <h2>Workflows</h2>
                </div>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="sidebar-wf-search">
                  <Search size={12} className="sidebar-wf-search-icon" />
                  <input
                    ref={sidebarSearchRef}
                    className="sidebar-wf-search-input"
                    type="text"
                    placeholder="Search..."
                    value={workflowSearch}
                    onChange={(e) => setWorkflowSearch(e.target.value)}
                  />
                  {workflowSearch && (
                    <button
                      className="sidebar-wf-search-clear"
                      onClick={() => { setWorkflowSearch(""); sidebarSearchRef.current?.focus(); }}
                      aria-label="Clear search"
                    >
                      <X size={10} />
                    </button>
                  )}
                </div>
                <div className="sidebar-section-body">
                  {filteredWorkflows.map((wf) => {
                    const wfEnabled = isWorkflowEnabled?.(wf.workspace, wf.name) ?? false;
                    return (
                    <div
                      key={wf.name}
                      className={`sidebar-item${editingWorkflow?.workspace === wf.workspace && editingWorkflow?.name === wf.name ? " active" : ""}${wfEnabled ? " sidebar-wf-enabled" : ""}`}
                      onClick={() => onSelectWorkflow(wf.workspace, wf.name)}
                    >
                      <div className="sidebar-item-row">
                        <div className="sidebar-item-name">{wf.name}</div>
                        <div className="sidebar-wf-actions">
                          <Switch
                            checked={wfEnabled}
                            onCheckedChange={() => onToggleWorkflowEnabled?.(wf.workspace, wf.name)}
                            onClick={(e) => e.stopPropagation()}
                            className="data-[state=checked]:bg-[var(--success)]"
                          />
                          <button
                            className="ghost sidebar-run-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              onRunWorkflow?.(wf.workspace, wf.name);
                            }}
                            title={wfEnabled ? "Run workflow" : "Run (manual)"}
                            aria-label="Run workflow"
                          >
                            <Play size={11} />
                          </button>
                          <button
                            className="ghost sidebar-delete-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              onDeleteWorkflow(wf.workspace, wf.name);
                            }}
                            title="Delete workflow"
                            aria-label="Delete workflow"
                          >
                            <X size={12} />
                          </button>
                        </div>
                      </div>
                      <div className="sidebar-item-meta sidebar-wf-meta">
                        <span>{wf.node_count} node{wf.node_count !== 1 ? "s" : ""}</span>
                        {wfEnabled && <span className="sidebar-wf-active-badge">Active</span>}
                      </div>
                    </div>
                    );
                  })}
                  {filteredWorkflows.length === 0 && (
                    <div className="sidebar-item-empty">
                      {deferredSearch ? `No matches for "${deferredSearch}"` : "No workflows in this workspace"}
                    </div>
                  )}
                </div>
              </CollapsibleContent>
            </Collapsible>
          )}
        </>
      ) : (
        <>
          {/* Projects section — collapsible project tree with nested agents */}
          <Collapsible defaultOpen className="sidebar-section">
            <CollapsibleTrigger asChild>
              <div className="sidebar-section-header">
                <span className="sidebar-chevron"><ChevronRight size={12} /></span>
                <h2>Projects</h2>
              </div>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="sidebar-section-body">
                <SidebarProjects
                  agents={agents}
                  onSelectAgent={onSelectAgent}
                  selectedAgentId={selectedAgentId}
                />
              </div>
            </CollapsibleContent>
          </Collapsible>

          {/* Agents section — Paperclip-style with live indicators (unassigned agents) */}
          <Collapsible defaultOpen className="sidebar-section">
            <CollapsibleTrigger asChild>
              <div className="sidebar-section-header">
                <span className="sidebar-chevron"><ChevronRight size={12} /></span>
                <h2>Agents</h2>
                <div style={{ flex: 1 }} />
                <button
                  className="ghost sidebar-action-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleSyncAgents();
                  }}
                  disabled={syncing}
                  aria-label="Sync agents"
                  title="Sync agents from repo"
                >
                  {syncing ? "..." : "↓"}
                </button>
                <button
                  className="ghost sidebar-action-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCreateAgent();
                  }}
                  aria-label="New agent"
                >
                  <Plus size={13} />
                </button>
              </div>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="sidebar-section-body">
                {unassignedAgents.map((agent) => {
                  const meta = agentMeta.get(agent.id);
                  const isActive = agent.id === selectedAgentId &&
                    (activeView === "agent-workspace" || activeView === "agent-detail");
                  const isBusy = meta?.busy ?? false;
                  const status = deriveAgentStatus(true, isBusy, false);
                  const dotColor = agentStatusDot[status] ?? agentStatusDotDefault;

                  return (
                    <div
                      key={agent.id}
                      className={`sb-agent-item${isActive ? " sb-agent-item-active" : ""}`}
                      onClick={() => onSelectAgent(agent.id)}
                    >
                      <div className="sb-agent-status-dot" style={{ background: dotColor }} />
                      <span className="sb-agent-name-text">{agent.name}</span>
                      {isBusy && (
                        <div className="sb-agent-live">
                          <div className="sb-agent-live-dot" />
                          <span className="sb-agent-live-text">Live</span>
                        </div>
                      )}
                      {agent.id !== STUDIO_ASSISTANT_ID && (
                        <button
                          className="ghost sb-agent-delete"
                          onClick={(e) => handleDeleteAgent(e, agent.id)}
                          title="Delete agent"
                          aria-label="Delete agent"
                        >
                          <X size={12} />
                        </button>
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

          {/* Prompts section */}
          <Collapsible defaultOpen className="sidebar-section">
            <CollapsibleTrigger asChild>
              <div className="sidebar-section-header">
                <span className="sidebar-chevron"><ChevronRight size={12} /></span>
                <h2>Prompts</h2>
                <div style={{ flex: 1 }} />
                <button
                  className="ghost sidebar-action-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCreatePrompt();
                  }}
                  aria-label="New prompt"
                >
                  <Plus size={13} />
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
                        aria-label="Delete prompt"
                       >
                        <X size={12} />
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
        </>
      )}

      {/* Node palette — visible in flow editor or when editing a workflow */}
      {(activeView === "flow-editor" || editingWorkflow) && activeFlowId && (
        <Collapsible defaultOpen className="sidebar-section sidebar-palette-section">
          <CollapsibleTrigger asChild>
            <div className="sidebar-section-header">
              <span className="sidebar-chevron"><ChevronRight size={12} /></span>
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

      <ConfirmDialog {...confirmState} />
      {showNewAgent && (
        <NewAgentDialog
          onClose={() => setShowNewAgent(false)}
          onCreated={handleAgentDialogCreated}
        />
      )}
    </div>
  );
}
