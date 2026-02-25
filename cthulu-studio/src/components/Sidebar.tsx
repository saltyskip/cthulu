import type { NodeTypeSchema } from "../types/flow";
import type { PromptFile } from "../api/client";

interface SidebarProps {
  nodeTypes: NodeTypeSchema[];
  onGrab: (nodeType: NodeTypeSchema) => void;
  promptFiles: PromptFile[];
  onSelectPrompt?: (file: PromptFile) => void;
}

const typeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  filter: "#ffa657",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Sidebar({ nodeTypes, onGrab, promptFiles, onSelectPrompt }: SidebarProps) {
  const grouped = {
    trigger: nodeTypes.filter((n) => n.node_type === "trigger"),
    source: nodeTypes.filter((n) => n.node_type === "source"),
    filter: nodeTypes.filter((n) => n.node_type === "filter"),
    executor: nodeTypes.filter((n) => n.node_type === "executor"),
    sink: nodeTypes.filter((n) => n.node_type === "sink"),
  };

  return (
    <div className="node-palette">
      <h3>Add Nodes</h3>
      {(["trigger", "source", "filter", "executor", "sink"] as const).map((type) => (
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

      {promptFiles.length > 0 && (
        <>
          <h3 style={{ marginTop: 16 }}>Prompts</h3>
          {promptFiles.map((pf) => (
            <div
              key={pf.path}
              className="palette-item prompt-file-item"
              onClick={() => onSelectPrompt?.(pf)}
              title={pf.path}
            >
              <div className="palette-dot" style={{ background: "var(--text-secondary)" }} />
              <span className="prompt-file-title">{pf.title}</span>
              <span className="prompt-file-path">{pf.path}</span>
            </div>
          ))}
        </>
      )}
    </div>
  );
}
