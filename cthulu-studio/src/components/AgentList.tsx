import { useState, useEffect, useCallback } from "react";
import type { AgentSummary } from "../types/flow";
import { listAgents, createAgent, deleteAgent } from "../api/client";

interface AgentListProps {
  onSelectAgent?: (agentId: string) => void;
  selectedAgentId?: string | null;
}

export default function AgentList({
  onSelectAgent,
  selectedAgentId,
}: AgentListProps) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);

  const refresh = useCallback(async () => {
    try {
      const list = await listAgents();
      setAgents(list);
    } catch {
      // Server may not be reachable yet
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function handleCreate() {
    try {
      const { id } = await createAgent({ name: "New Agent" });
      await refresh();
      onSelectAgent?.(id);
    } catch (e) {
      console.error("Failed to create agent:", e);
    }
  }

  async function handleDelete(e: React.MouseEvent, agentId: string) {
    e.stopPropagation();
    if (!confirm("Delete this agent?")) return;
    try {
      await deleteAgent(agentId);
      await refresh();
    } catch (err) {
      console.error("Failed to delete agent:", err);
    }
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        overflow: "hidden",
      }}
    >
      <div className="sidebar-header" style={{ flexShrink: 0 }}>
        <h2>Agents</h2>
        <button className="ghost" onClick={handleCreate}>
          + New
        </button>
      </div>
      <div className="flow-list">
        {agents.map((agent) => (
          <div
            key={agent.id}
            className={`flow-item${agent.id === selectedAgentId ? " active" : ""}`}
            onClick={() => onSelectAgent?.(agent.id)}
          >
            <div className="flow-item-row">
              <div className="flow-item-name">{agent.name}</div>
              <button
                className="ghost"
                onClick={(e) => handleDelete(e, agent.id)}
                style={{ fontSize: 10, opacity: 0.5, padding: "0 4px" }}
                title="Delete agent"
              >
                ×
              </button>
            </div>
            {agent.description && (
              <div className="flow-item-meta">{agent.description}</div>
            )}
            {agent.permissions.length > 0 && (
              <div className="flow-item-meta">
                {agent.permissions.length} permission
                {agent.permissions.length !== 1 ? "s" : ""}
              </div>
            )}
          </div>
        ))}
        {agents.length === 0 && (
          <div className="flow-item">
            <div className="flow-item-meta">No agents yet</div>
          </div>
        )}
      </div>
    </div>
  );
}
