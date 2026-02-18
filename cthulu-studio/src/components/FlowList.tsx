import type { FlowSummary } from "../types/flow";

interface FlowListProps {
  flows: FlowSummary[];
  activeFlowId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
}

export default function FlowList({
  flows,
  activeFlowId,
  onSelect,
  onCreate,
}: FlowListProps) {
  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <h2>Flows</h2>
        <button className="ghost" onClick={onCreate}>
          + New
        </button>
      </div>
      <div className="flow-list">
        {flows.map((flow) => (
          <div
            key={flow.id}
            className={`flow-item ${flow.id === activeFlowId ? "active" : ""}`}
            onClick={() => onSelect(flow.id)}
          >
            <div className="flow-item-name">{flow.name}</div>
            <div className="flow-item-meta">
              {flow.node_count} nodes &middot;{" "}
              {flow.enabled ? "Enabled" : "Disabled"}
            </div>
          </div>
        ))}
        {flows.length === 0 && (
          <div className="flow-item">
            <div className="flow-item-meta">No flows yet</div>
          </div>
        )}
      </div>
    </div>
  );
}
