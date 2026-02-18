import { Handle, Position } from "@xyflow/react";

interface TriggerNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
}

export default function TriggerNode({ data }: { data: TriggerNodeData }) {
  return (
    <div className="custom-node">
      <div className="node-header">
        <span className="node-type-badge trigger">Trigger</span>
      </div>
      <div className="node-label">{data.label}</div>
      <div className="node-kind">{data.kind}</div>
      <Handle id="out" type="source" position={Position.Right} />
    </div>
  );
}
