import { Handle, Position } from "@xyflow/react";

interface ExecutorNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
  runStatus?: "running" | "completed" | "failed" | null;
  validationErrors?: string[];
}

function runtimeLabel(config: Record<string, unknown>, kind: string): string {
  const runtime = (config.runtime as string) || kind;
  switch (runtime) {
    case "sandbox":
      return "Sandbox";
    default:
      return "Local";
  }
}

export default function ExecutorNode({ data }: { data: ExecutorNodeData }) {
  const agentName = data.config?.agent_name as string | undefined;
  const agentId = data.config?.agent_id as string | undefined;
  const hasAgent = agentId && agentId.length > 0;
  const runtime = runtimeLabel(data.config || {}, data.kind);

  return (
    <div className={`custom-node${data.runStatus ? ` run-${data.runStatus}` : ""}`}>
      <Handle id="in" type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge executor">
          {hasAgent ? "Agent" : "Executor"}
        </span>
        <span
          className="node-runtime-badge"
          style={{
            fontSize: 9,
            padding: "1px 4px",
            borderRadius: 3,
            background: "var(--bg)",
            color: "var(--text-secondary)",
            marginLeft: 4,
          }}
        >
          {runtime}
        </span>
        {data.validationErrors && data.validationErrors.length > 0 && (
          <span className="node-validation-badge" title={data.validationErrors.join("\n")}>!</span>
        )}
      </div>
      <div className="node-label">{data.label}</div>
      {hasAgent ? (
        <div className="node-kind" style={{ color: "var(--accent)" }}>{agentName || agentId}</div>
      ) : (
        <div className="node-kind" style={{ color: "var(--warning)" }}>No agent selected</div>
      )}
      <Handle id="out" type="source" position={Position.Right} />
    </div>
  );
}
