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

const TerminalIcon = () => (
  <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
    <path d="M2.146 4.854a.5.5 0 0 1 .708-.708l3 3a.5.5 0 0 1 0 .708l-3 3a.5.5 0 0 1-.708-.708L4.793 7.5 2.146 4.854zM7.5 10.5a.5.5 0 0 0 0 1h4a.5.5 0 0 0 0-1h-4z" />
  </svg>
);

function runtimeLabel(config: Record<string, unknown>, kind: string): string {
  const runtime = (config.runtime as string) || kind;
  switch (runtime) {
    case "sandbox":
      return "Sandbox";
    case "vm-sandbox":
      return "VM";
    default:
      return "Local";
  }
}

export default function ExecutorNode({ data }: { data: ExecutorNodeData }) {
  const isSandboxed = data.kind === "vm-sandbox" || data.config?.runtime === "vm-sandbox";
  const agentName = data.config?.agent_name as string | undefined;
  const runtime = runtimeLabel(data.config || {}, data.kind);

  return (
    <div className={`custom-node${data.runStatus ? ` run-${data.runStatus}` : ""}${isSandboxed ? " sandboxed" : ""}`}>
      <Handle id="in" type="target" position={Position.Left} />
      <div className="node-header">
        <span className="node-type-badge executor">
          {agentName ? "Agent" : "Executor"}
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
        <button
          className="node-step-in"
          title="Open terminal"
          style={{
            marginLeft: "auto",
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-secondary)",
            padding: "0 2px",
            display: "flex",
            alignItems: "center",
          }}
          onClick={(e) => {
            e.stopPropagation();
            // Dispatch custom event to open the bottom panel terminal tab
            window.dispatchEvent(
              new CustomEvent("cthulu:step-in", { detail: { nodeId: data.config?.nodeId || "" } })
            );
          }}
        >
          <TerminalIcon />
        </button>
      </div>
      <div className="node-label">{data.label}</div>
      {agentName ? (
        <div className="node-kind" style={{ color: "var(--accent)" }}>{agentName}</div>
      ) : (
        <div className="node-kind">{data.kind}</div>
      )}
      {isSandboxed && <LockIcon />}
      <Handle id="out" type="source" position={Position.Right} />
    </div>
  );
}
