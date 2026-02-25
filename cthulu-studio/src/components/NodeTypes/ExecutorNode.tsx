import { Handle, Position } from "@xyflow/react";

interface ExecutorNodeData {
  label: string;
  kind: string;
  config: Record<string, unknown>;
  runStatus?: "running" | "completed" | "failed" | null;
  validationErrors?: string[];
}

const LockIcon = () => (
  <svg
    className="node-sandbox-badge"
    viewBox="0 0 16 16"
    width="14"
    height="14"
    fill="currentColor"
    aria-label="Sandboxed"
  >
    <path d="M8 1a4 4 0 0 0-4 4v2H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-1V5a4 4 0 0 0-4-4zm-2.5 4a2.5 2.5 0 1 1 5 0v2h-5V5z" />
  </svg>
);

export default function ExecutorNode({ data }: { data: ExecutorNodeData }) {
  const isSandboxed = data.kind === "vm-sandbox";

  return (
    <div className={`custom-node${data.runStatus ? ` run-${data.runStatus}` : ""}${isSandboxed ? " sandboxed" : ""}`}>
      <Handle id="in" type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge executor">
          {isSandboxed ? "Agent" : "Executor"}
        </span>
        {data.validationErrors && data.validationErrors.length > 0 && (
          <span className="node-validation-badge" title={data.validationErrors.join("\n")}>!</span>
        )}
      </div>
      <div className="node-label">{data.label}</div>
      <div className="node-kind">{data.kind}</div>
      {isSandboxed && <LockIcon />}
      <Handle id="out" type="source" position={Position.Right} />
    </div>
  );
}
