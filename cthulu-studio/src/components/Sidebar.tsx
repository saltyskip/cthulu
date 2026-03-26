import type { FlowSummary, Flow, NodeTypeSchema, ActiveView } from "../types/flow";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import AgentList from "./AgentList";
import FlowList from "./FlowList";
import PromptList from "./PromptList";
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

      <AgentList
        selectedAgentId={selectedAgentId}
        selectedSessionId={selectedSessionId}
        onSelectSession={onSelectSession}
        agentListKey={agentListKey}
        onAgentCreated={onAgentCreated}
        activeView={activeView}
      />

      <FlowList
        flows={flows}
        activeFlowId={activeFlowId}
        onSelectFlow={onSelectFlow}
        onCreateFlow={onCreateFlow}
        onImportTemplate={onImportTemplate}
        onToggleEnabled={onToggleEnabled}
        activeView={activeView}
      />

      <PromptList
        selectedPromptId={selectedPromptId}
        onSelectPrompt={onSelectPrompt}
        promptListKey={promptListKey}
        activeView={activeView}
      />

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
