import type { NodeTypeSchema } from "../types/flow";

interface SidebarProps {
  nodeTypes: NodeTypeSchema[];
  onDragStart: (event: React.DragEvent, nodeType: NodeTypeSchema) => void;
}

const typeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Sidebar({ nodeTypes, onDragStart }: SidebarProps) {
  const grouped = {
    trigger: nodeTypes.filter((n) => n.node_type === "trigger"),
    source: nodeTypes.filter((n) => n.node_type === "source"),
    executor: nodeTypes.filter((n) => n.node_type === "executor"),
    sink: nodeTypes.filter((n) => n.node_type === "sink"),
  };

  return (
    <div className="node-palette">
      <h3>Add Nodes</h3>
      {(["trigger", "source", "executor", "sink"] as const).map((type) => (
        <div key={type}>
          {grouped[type].map((nt) => (
            <div
              key={nt.kind}
              className="palette-item"
              draggable
              onDragStart={(e) => onDragStart(e, nt)}
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
  );
}
