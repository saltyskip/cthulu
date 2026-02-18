import { Handle, Position } from "@xyflow/react";

interface ExecutorNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
}

export default function ExecutorNode({ data }: { data: ExecutorNodeData }) {
  return (
    <div className="custom-node">
      <Handle type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge executor">Executor</span>
      </div>
      <div className="node-label">{data.label}</div>
      <div className="node-kind">{data.kind}</div>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
