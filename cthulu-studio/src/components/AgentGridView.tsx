import { useState, useEffect, useCallback } from "react";
import { listAgents, listAgentSessions, createAgent } from "../api/client";
import type { AgentSummary } from "../types/flow";
import type { AgentSessionsInfo } from "../api/client";

interface AgentGridViewProps {
  onSelectAgent: (id: string) => void;
  onCreateAgent: () => void;
  agentListKey: number;
}

interface AgentCardData {
  agent: AgentSummary;
  sessionCount: number;
  busy: boolean;
}

function formatTimeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 0) return "just now";
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

export default function AgentGridView({
  onSelectAgent,
  onCreateAgent,
  agentListKey,
}: AgentGridViewProps) {
  const [cards, setCards] = useState<AgentCardData[]>([]);
  const [loading, setLoading] = useState(true);

  const loadData = useCallback(async () => {
    try {
      const agents = await listAgents();
      const sessionResults = await Promise.allSettled(
        agents.map((a) => listAgentSessions(a.id))
      );

      const data: AgentCardData[] = agents.map((agent, i) => {
        const result = sessionResults[i];
        let sessionCount = 0;
        let busy = false;
        if (result.status === "fulfilled") {
          const info: AgentSessionsInfo = result.value;
          sessionCount = info.sessions.length;
          busy = info.sessions.some((s) => s.busy);
        }
        return { agent, sessionCount, busy };
      });
      setCards(data);
    } catch {
      // server may be unreachable
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData, agentListKey]);

  return (
    <div className="agent-grid-container">
      <div className="agent-grid-header">
        <h2>Agents</h2>
      </div>
      {loading ? (
        <div className="agent-grid-loading">Loading agents...</div>
      ) : (
        <div className="agent-grid">
          {cards.map(({ agent, sessionCount, busy }) => (
            <div
              key={agent.id}
              className="agent-card"
              onClick={() => onSelectAgent(agent.id)}
            >
              <div className="agent-card-name">{agent.name}</div>
              {agent.description && (
                <div className="agent-card-desc">{agent.description}</div>
              )}
              <div className="agent-card-footer">
                <div className="agent-card-status">
                  {busy && <span className="agent-card-busy-dot" />}
                  {sessionCount > 0 && (
                    <span>{sessionCount} session{sessionCount !== 1 ? "s" : ""}</span>
                  )}
                </div>
                <span className="agent-card-time">
                  {formatTimeAgo(agent.updated_at)}
                </span>
              </div>
            </div>
          ))}
          <div
            className="agent-card agent-card-create"
            onClick={onCreateAgent}
          >
            <div className="agent-card-create-icon">+</div>
            <div className="agent-card-create-label">Create New Agent</div>
          </div>
        </div>
      )}
    </div>
  );
}
