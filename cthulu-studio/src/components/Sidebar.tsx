import { useState, useEffect, useCallback } from "react";
import { STUDIO_ASSISTANT_ID, type FlowSummary, type Flow, type NodeTypeSchema, type AgentSummary } from "../types/flow";
import { listAgents, createAgent, deleteAgent } from "../api/client";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import TemplateGallery from "./TemplateGallery";

type ActiveView = "flow-editor" | "agent-workspace";

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
  agentListKey: number;
  onAgentCreated: (id: string) => void;
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
  agentListKey,
  onAgentCreated,
  activeView,
  nodeTypes,
  onGrab,
}: SidebarProps) {
  const [showGallery, setShowGallery] = useState(false);
  const [agents, setAgents] = useState<AgentSummary[]>([]);

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
            <h2>Agents</h2>
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
                  <div className="sidebar-item-name">{agent.name}</div>
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
