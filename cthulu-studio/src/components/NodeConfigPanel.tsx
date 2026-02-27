import { useState, useEffect, useCallback, useRef } from "react";
import type { Flow, AgentSummary } from "../types/flow";
import type { CanvasHandle } from "./Canvas";
import { listAgents } from "../api/client";

interface NodeConfigPanelProps {
  nodeId: string;
  canonicalFlow: Flow;
  canvasRef: React.RefObject<CanvasHandle | null>;
}

export default function NodeConfigPanel({
  nodeId,
  canonicalFlow,
  canvasRef,
}: NodeConfigPanelProps) {
  const node = canonicalFlow.nodes.find((n) => n.id === nodeId);
  if (!node) return null;

  if (node.node_type === "executor") {
    return (
      <ExecutorConfigPanel
        nodeId={nodeId}
        config={node.config}
        canvasRef={canvasRef}
      />
    );
  }

  return (
    <div className="node-config-panel">
      <div className="node-config-header">
        <span className="node-config-title">{node.label}</span>
        <span className="node-config-kind">{node.kind}</span>
      </div>
      <div className="node-config-body">
        <p style={{ color: "var(--text-secondary)", fontSize: 12 }}>
          Edit this node's config in the JSON editor.
        </p>
      </div>
    </div>
  );
}

function ExecutorConfigPanel({
  nodeId,
  config,
  canvasRef,
}: {
  nodeId: string;
  config: Record<string, unknown>;
  canvasRef: React.RefObject<CanvasHandle | null>;
}) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const fetchedRef = useRef(false);

  useEffect(() => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;
    listAgents()
      .then(setAgents)
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const agentId = (config.agent_id as string) || "";
  const prompt = (config.prompt as string) || "";
  const workingDir = (config.working_dir as string) || "";

  const updateConfig = useCallback(
    (updates: Record<string, unknown>) => {
      canvasRef.current?.updateNodeData(nodeId, {
        config: { ...config, ...updates },
      });
    },
    [nodeId, config, canvasRef]
  );

  const handleAgentChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const selectedId = e.target.value;
      const agent = agents.find((a) => a.id === selectedId);
      updateConfig({
        agent_id: selectedId,
        agent_name: agent?.name || "",
      });
    },
    [agents, updateConfig]
  );

  return (
    <div className="node-config-panel">
      <div className="node-config-header">
        <span className="node-config-title">Executor Config</span>
      </div>
      <div className="node-config-body">
        <label className="node-config-label">
          Agent <span style={{ color: "var(--danger)" }}>*</span>
        </label>
        {loading ? (
          <div style={{ fontSize: 12, color: "var(--text-secondary)", padding: "4px 0" }}>
            Loading agents...
          </div>
        ) : (
          <select
            className="node-config-select"
            value={agentId}
            onChange={handleAgentChange}
          >
            <option value="">-- Select an agent --</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        )}
        {!agentId && !loading && (
          <span className="field-error">Select an agent for this executor</span>
        )}

        <label className="node-config-label" style={{ marginTop: 12 }}>
          Prompt
        </label>
        <input
          className="node-config-input"
          type="text"
          placeholder="Prompt file path or inline prompt"
          value={prompt}
          onChange={(e) => updateConfig({ prompt: e.target.value })}
        />

        <label className="node-config-label" style={{ marginTop: 12 }}>
          Working Directory
        </label>
        <input
          className="node-config-input"
          type="text"
          placeholder="."
          value={workingDir}
          onChange={(e) => updateConfig({ working_dir: e.target.value })}
        />
      </div>
    </div>
  );
}
