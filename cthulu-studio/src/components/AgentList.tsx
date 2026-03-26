import { useState, useEffect, useCallback } from "react";
import { STUDIO_ASSISTANT_ID, type AgentSummary, type ActiveView } from "../types/flow";
import { listAgents, createAgent, deleteAgent, listAgentSessions, newAgentSession } from "../api/client";
import type { InteractSessionInfo } from "../api/client";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";

interface AgentListProps {
  selectedAgentId: string | null;
  selectedSessionId: string | null;
  onSelectSession: (agentId: string, sessionId: string) => void;
  agentListKey: number;
  onAgentCreated: (id: string) => void;
  activeView: ActiveView;
}

export default function AgentList({
  selectedAgentId,
  selectedSessionId,
  onSelectSession,
  agentListKey,
  onAgentCreated,
  activeView,
}: AgentListProps) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [agentMeta, setAgentMeta] = useState<Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>>(new Map());
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set());

  const refreshAgents = useCallback(async () => {
    try {
      const list = await listAgents();
      setAgents(list.filter((a) => !a.subagent_only));
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshAgents();
  }, [refreshAgents, agentListKey]);

  useEffect(() => {
    if (agents.length === 0) return;

    const fetchMeta = async () => {
      const results = await Promise.allSettled(
        agents.map((a) => listAgentSessions(a.id).then((info) => ({ id: a.id, info })))
      );
      const next = new Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>();
      for (const r of results) {
        if (r.status === "fulfilled") {
          const { id, info } = r.value;
          const busy = info.sessions.some((s) => s.busy);
          const cost = info.sessions.reduce((sum, s) => sum + s.total_cost, 0);
          next.set(id, { busy, sessions: info.sessions, cost });
        }
      }
      setAgentMeta(next);
    };

    fetchMeta();
    const interval = setInterval(fetchMeta, 5000);
    return () => clearInterval(interval);
  }, [agents]);

  async function handleCreateAgent() {
    try {
      const { id } = await createAgent({ name: "New Agent" });
      await refreshAgents();
      onAgentCreated(id);
    } catch (e) {
      console.error("Failed to create agent:", e);
    }
  }

  async function handleDeleteAgent(e: React.MouseEvent, agentId: string) {
    e.stopPropagation();
    if (!confirm("Delete this agent?")) return;
    try {
      await deleteAgent(agentId);
      await refreshAgents();
    } catch (err) {
      console.error("Failed to delete agent:", err);
    }
  }

  return (
    <Collapsible defaultOpen className="sidebar-section">
      <CollapsibleTrigger asChild>
        <div className="sidebar-section-header">
          <span className="sidebar-chevron">▶</span>
          <h2>Agents</h2>
          <div style={{ flex: 1 }} />
          <button
            className="ghost sidebar-action-btn"
            onClick={(e) => {
              e.stopPropagation();
              handleCreateAgent();
            }}
          >
            +
          </button>
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="sidebar-section-body">
          {[...agents].sort((a, b) => {
            if (a.id === STUDIO_ASSISTANT_ID) return -1;
            if (b.id === STUDIO_ASSISTANT_ID) return 1;
            return 0;
          }).map((agent) => {
            const meta = agentMeta.get(agent.id);
            const isExpanded = expandedAgents.has(agent.id);
            const isActive = agent.id === selectedAgentId && activeView === "agent-workspace";
            const sessions = meta?.sessions ?? [];

            return (
              <div key={agent.id} className="sb-agent">
                <div
                  className={`sb-agent-row${isActive ? " sb-agent-active" : ""}`}
                  onClick={() => {
                    setExpandedAgents((prev) => {
                      const next = new Set(prev);
                      if (next.has(agent.id)) next.delete(agent.id);
                      else next.add(agent.id);
                      return next;
                    });
                    if (sessions.length > 0) {
                      onSelectSession(agent.id, sessions[0].session_id);
                    }
                  }}
                >
                  <span className="sb-agent-chevron">{isExpanded ? "▾" : "▸"}</span>
                  {meta?.busy && <span className="sb-agent-pulse" />}
                  <span className="sb-agent-name">{agent.name}</span>
                  {meta && meta.cost > 0 && (
                    <span className="sb-agent-cost">${meta.cost.toFixed(2)}</span>
                  )}
                  {agent.id !== STUDIO_ASSISTANT_ID && (
                    <button
                      className="ghost sb-agent-delete"
                      onClick={(e) => handleDeleteAgent(e, agent.id)}
                      title="Delete agent"
                    >
                      ×
                    </button>
                  )}
                </div>
                {isExpanded && (
                  <div className="sb-sessions">
                    {sessions.map((s) => {
                      const isSessionActive = s.session_id === selectedSessionId && agent.id === selectedAgentId;
                      const label = s.summary || (s.kind === "flow_run" ? `Run: ${s.flow_run?.flow_name ?? ""}` : "New session");
                      return (
                        <div
                          key={s.session_id}
                          className={`sb-session${isSessionActive ? " sb-session-active" : ""}`}
                          onClick={() => onSelectSession(agent.id, s.session_id)}
                        >
                          {s.busy && <span className="sb-session-pulse" />}
                          <span className="sb-session-label">{label}</span>
                          {s.total_cost > 0 && (
                            <span className="sb-session-cost">${s.total_cost.toFixed(2)}</span>
                          )}
                        </div>
                      );
                    })}
                    <button
                      className="sb-session-new"
                      onClick={async (e) => {
                        e.stopPropagation();
                        try {
                          const result = await newAgentSession(agent.id);
                          onSelectSession(agent.id, result.session_id);
                        } catch (err) {
                          console.error("Failed to create session:", err);
                        }
                      }}
                    >
                      + New Session
                    </button>
                  </div>
                )}
              </div>
            );
          })}
          {agents.length === 0 && (
            <div className="sidebar-item-empty">No agents yet</div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
