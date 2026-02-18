import { Handle, Position } from "@xyflow/react";

interface SinkNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
}

export default function SinkNode({ data }: { data: SinkNodeData }) {
  return (
    <div className="custom-node">
      <Handle id="in" type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge sink">Sink</span>
      </div>
      <div className="node-label">{data.label}</div>
      <div className="node-kind">{data.kind}</div>
    </div>
  );
}
