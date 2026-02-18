import { Handle, Position } from "@xyflow/react";

interface SourceNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
}

export default function SourceNode({ data }: { data: SourceNodeData }) {
  return (
    <div className="custom-node">
      <Handle id="in" type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge source">Source</span>
      </div>
      <div className="node-label">{data.label}</div>
      <div className="node-kind">{data.kind}</div>
      <Handle id="out" type="source" position={Position.Right} />
    </div>
  );
}
