import { useState, useEffect, useCallback } from "react";
import { STUDIO_ASSISTANT_ID, type FlowSummary, type Flow, type NodeTypeSchema, type AgentSummary, type SavedPrompt } from "../types/flow";
import { listAgents, createAgent, deleteAgent, listPrompts, savePrompt, deletePrompt as deletePromptApi, listAgentSessions } from "../api/client";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import TemplateGallery from "./TemplateGallery";

type ActiveView = "flow-editor" | "agent-grid" | "agent-workspace" | "prompt-editor";

interface SidebarProps {
  // Flow list
  flows: FlowSummary[];
  activeFlowId: string | null;
  onSelectFlow: (id: string) => void;
  onCreateFlow: () => void;
  onImportTemplate: (flow: Flow) => void;
  onToggleEnabled: (flowId: string) => void;
  // Agent list
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  onShowAgentGrid: () => void;
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
  onSelectAgent,
  onShowAgentGrid,
  agentListKey,
  onAgentCreated,
  selectedPromptId,
  onSelectPrompt,
  promptListKey,
  activeView,
  nodeTypes,
  onGrab,
}: SidebarProps) {
  const [showGallery, setShowGallery] = useState(false);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [agentMeta, setAgentMeta] = useState<Map<string, { busy: boolean; sessions: number; cost: number }>>(new Map());

  const refreshAgents = useCallback(async () => {
    try {
      const list = await listAgents();
      setAgents(list);
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshAgents();
  }, [refreshAgents, agentListKey]);

  // Poll agent session metadata for status indicators
  useEffect(() => {
    if (agents.length === 0) return;

    const fetchMeta = async () => {
      const results = await Promise.allSettled(
        agents.map((a) => listAgentSessions(a.id).then((info) => ({ id: a.id, info })))
      );
      const next = new Map<string, { busy: boolean; sessions: number; cost: number }>();
      for (const r of results) {
        if (r.status === "fulfilled") {
          const { id, info } = r.value;
          const busy = info.sessions.some((s) => s.busy);
          const cost = info.sessions.reduce((sum, s) => sum + s.total_cost, 0);
          next.set(id, { busy, sessions: info.sessions.length, cost });
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
      {showGallery && (
        <TemplateGallery
          onImport={handleGalleryImport}
          onBlank={handleBlank}
          onClose={() => setShowGallery(false)}
        />
      )}

      {/* Flows section */}
      <Collapsible defaultOpen className="sidebar-section">
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

      {/* Agents section */}
      <Collapsible defaultOpen className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2
              onClick={(e) => {
                e.stopPropagation();
                onShowAgentGrid();
              }}
              style={{ cursor: "pointer" }}
              title="View all agents"
            >
              Agents
            </h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleCreateAgent();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {[...agents].sort((a, b) => {
              if (a.id === STUDIO_ASSISTANT_ID) return -1;
              if (b.id === STUDIO_ASSISTANT_ID) return 1;
              return 0;
            }).map((agent) => (
              <div
                key={agent.id}
                className={`sidebar-item${agent.id === selectedAgentId && activeView === "agent-workspace" ? " active" : ""}`}
                onClick={() => onSelectAgent(agent.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">
                    {agentMeta.get(agent.id)?.busy && (
                      <span className="sidebar-agent-busy" />
                    )}
                    {agent.name}
                  </div>
                  {agent.id !== STUDIO_ASSISTANT_ID && (
                    <button
                      className="ghost sidebar-delete-btn"
                      onClick={(e) => handleDeleteAgent(e, agent.id)}
                      title="Delete agent"
                    >
                      ×
                    </button>
                  )}
                </div>
                {agentMeta.has(agent.id) && (
                  <div className="sidebar-agent-meta">
                    {agentMeta.get(agent.id)!.sessions}s
                    {agentMeta.get(agent.id)!.cost > 0 && (
                      <> &middot; ${agentMeta.get(agent.id)!.cost.toFixed(2)}</>
                    )}
                  </div>
                )}
                {agent.description && (
                  <div className="sidebar-item-meta">{agent.description}</div>
                )}
              </div>
            ))}
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
