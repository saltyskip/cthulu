import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "../api/client";
import type { Agent, AgentSummary } from "../types/flow";
import { STUDIO_ASSISTANT_ID } from "../types/flow";
import { StatusBadge } from "./StatusBadge";
import { AgentDashboard } from "./AgentDashboard";
import { AgentConfigPage } from "./AgentConfigPage";
import { AgentRunsTab } from "./AgentRunsTab";
import { TaskList } from "./TaskList";
import { deriveAgentStatus } from "../lib/status-colors";
import { LayoutDashboard, Settings, Play, ClipboardList } from "lucide-react";

type DetailTab = "dashboard" | "configuration" | "runs" | "tasks";

interface AgentDetailPageProps {
  agentId: string;
  sessionId: string;
  onBack: () => void;
  onDeleted: () => void;
}

export function AgentDetailPage({ agentId, sessionId, onBack, onDeleted }: AgentDetailPageProps) {
  const [agent, setAgent] = useState<Agent | null>(null);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [tab, setTab] = useState<DetailTab>("dashboard");
  const [sessionBusy, setSessionBusy] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close menu on outside click
  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  // Load agent
  const loadAgent = useCallback(() => {
    api.getAgent(agentId).then(setAgent).catch(() => {});
  }, [agentId]);

  useEffect(() => { loadAgent(); }, [loadAgent]);

  // Load agents list (for Tasks tab)
  useEffect(() => {
    api.listAgents().then(setAgents).catch(() => {});
  }, []);

  // Poll session status
  useEffect(() => {
    const poll = () => {
      api.listAgentSessions(agentId).then(res => {
        const sessions = res.sessions ?? [];
        setSessionBusy(sessions.some((s: any) => s.busy));
      }).catch(() => {});
    };
    poll();
    const iv = setInterval(poll, 5000);
    return () => clearInterval(iv);
  }, [agentId]);

  const handleTriggerHeartbeat = useCallback(async () => {
    try { await api.wakeupAgent(agentId); } catch (e) { console.error(e); }
  }, [agentId]);

  const handleDelete = useCallback(async () => {
    if (!confirm(`Delete agent "${agent?.name}"?`)) return;
    try {
      await api.deleteAgent(agentId);
      onDeleted();
    } catch (e) { console.error(e); }
  }, [agentId, agent?.name, onDeleted]);

  if (!agent) {
    return <div className="agent-detail-loading">Loading...</div>;
  }

  const status = deriveAgentStatus(agent.heartbeat_enabled, sessionBusy, false);
  const isStudioAssistant = agent.id === STUDIO_ASSISTANT_ID;

  return (
    <div className="agent-detail-page">
      {/* Header */}
      <div className="agent-detail-page-header">
        <button className="agent-detail-back" onClick={onBack}>
          ← Back
        </button>
        <div className="agent-detail-identity">
          <h2 className="agent-detail-name">{agent.name}</h2>
          {agent.description && (
            <p className="agent-detail-desc">{agent.description}</p>
          )}
        </div>
        <div className="agent-detail-header-actions">
          <button className="agent-detail-action-btn" onClick={handleTriggerHeartbeat} title="Trigger heartbeat run">
            ▶ Run Heartbeat
          </button>
          <StatusBadge status={status} />
          {sessionBusy && (
            <span className="live-indicator">
              <span className="live-indicator-dot" />
              Live
            </span>
          )}
          {!isStudioAssistant && (
            <div className="agent-detail-overflow" ref={menuRef}>
              <button className="agent-detail-overflow-btn" onClick={() => setMenuOpen(!menuOpen)}>
                ⋯
              </button>
              {menuOpen && (
                <div className="agent-detail-overflow-menu">
                  <button onClick={() => { navigator.clipboard.writeText(agent.id); setMenuOpen(false); }}>
                    Copy Agent ID
                  </button>
                  <button className="agent-detail-overflow-danger" onClick={() => { handleDelete(); setMenuOpen(false); }}>
                    Delete Agent
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Tab Bar */}
      <div className="agent-detail-tab-bar">
        {([
          { id: "dashboard" as DetailTab, label: "Dashboard", icon: LayoutDashboard },
          { id: "configuration" as DetailTab, label: "Configuration", icon: Settings },
          { id: "runs" as DetailTab, label: "Runs", icon: Play },
          { id: "tasks" as DetailTab, label: "Tasks", icon: ClipboardList },
        ]).map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            className={`agent-detail-tab${tab === id ? " agent-detail-tab-active" : ""}`}
            onClick={() => setTab(id)}
          >
            <Icon className="agent-detail-tab-icon" />
            {label}
          </button>
        ))}
      </div>

      {/* Tab Content */}
      <div className="agent-detail-tab-content">
        {tab === "dashboard" && <AgentDashboard agent={agent} sessionId={sessionId} />}
        {tab === "configuration" && <AgentConfigPage agent={agent} onAgentUpdated={loadAgent} />}
        {tab === "runs" && <AgentRunsTab agentId={agentId} />}
        {tab === "tasks" && <TaskList agentId={agentId} agents={agents} />}
      </div>
    </div>
  );
}
