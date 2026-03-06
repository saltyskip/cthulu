import { useState, useEffect, useCallback } from "react";
import {
  getAgent,
  getSessionStatus,
  killSession,
  deleteAgentSession,
  type SessionStatus,
} from "../api/client";
import type { Agent } from "../types/flow";

interface SessionInfoPanelProps {
  agentId: string;
  sessionId: string;
  onSessionDeleted: () => void;
}

export default function SessionInfoPanel({
  agentId,
  sessionId,
  onSessionDeleted,
}: SessionInfoPanelProps) {
  const [agent, setAgent] = useState<Agent | null>(null);
  const [status, setStatus] = useState<SessionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [killing, setKilling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Fetch agent + session status on mount and periodically
  useEffect(() => {
    let cancelled = false;
    const fetchAll = async () => {
      try {
        const [a, s] = await Promise.all([
          getAgent(agentId),
          getSessionStatus(agentId, sessionId),
        ]);
        if (cancelled) return;
        setAgent(a);
        setStatus(s);
        setError(null);
      } catch {
        if (!cancelled) setError("Failed to load session info");
      }
    };
    fetchAll();
    const interval = setInterval(fetchAll, 5000);
    return () => { cancelled = true; clearInterval(interval); };
  }, [agentId, sessionId]);

  const handleKill = useCallback(async () => {
    setKilling(true);
    try {
      await killSession(agentId, sessionId);
      const s = await getSessionStatus(agentId, sessionId);
      setStatus(s);
    } catch {
      setError("Failed to kill session");
    } finally {
      setKilling(false);
    }
  }, [agentId, sessionId]);

  const handleDelete = useCallback(async () => {
    setDeleting(true);
    try {
      await deleteAgentSession(agentId, sessionId);
      onSessionDeleted();
    } catch {
      setError("Failed to delete session");
      setDeleting(false);
      setConfirmDelete(false);
    }
  }, [agentId, sessionId, onSessionDeleted]);

  if (error && !agent && !status) {
    return (
      <div className="session-info-panel">
        <div className="session-info-error">{error}</div>
      </div>
    );
  }

  return (
    <div className="session-info-panel">
      {/* ── Session Status ── */}
      <section className="session-info-section">
        <h3 className="session-info-heading">Session</h3>
        {status ? (
          <div className="session-info-grid">
            <Row label="Status">
              <StatusDot alive={status.process_alive} busy={status.busy} />
              {status.busy
                ? "Busy"
                : status.process_alive
                  ? "Idle"
                  : "Stopped"}
            </Row>
            <Row label="Messages">{status.message_count}</Row>
            <Row label="Cost">
              {status.total_cost > 0
                ? `$${status.total_cost.toFixed(4)}`
                : "—"}
            </Row>
            {status.busy_since && (
              <Row label="Busy since">
                {new Date(status.busy_since).toLocaleTimeString()}
              </Row>
            )}
            <Row label="Session ID">
              <span className="session-info-mono session-info-truncate">
                {sessionId.slice(0, 12)}
              </span>
            </Row>
          </div>
        ) : (
          <div className="session-info-loading">Loading...</div>
        )}
      </section>

      {/* ── Agent Config ── */}
      {agent && (
        <section className="session-info-section">
          <h3 className="session-info-heading">Agent</h3>
          <div className="session-info-grid">
            <Row label="Name">{agent.name}</Row>
            {agent.working_dir && (
              <Row label="Working dir">
                <span className="session-info-mono session-info-truncate">
                  {agent.working_dir}
                </span>
              </Row>
            )}
            <Row label="Permissions">
              {agent.permissions.length > 0 ? (
                <div className="session-info-tags">
                  {agent.permissions.map((p) => (
                    <span key={p} className="session-info-tag">{p}</span>
                  ))}
                </div>
              ) : (
                <span className="session-info-muted">None (default-deny)</span>
              )}
            </Row>
            {agent.prompt && (
              <Row label="Prompt">
                <span className="session-info-muted session-info-truncate">
                  {agent.prompt.length > 120
                    ? agent.prompt.slice(0, 120) + "..."
                    : agent.prompt}
                </span>
              </Row>
            )}
            {agent.append_system_prompt && (
              <Row label="System append">
                <span className="session-info-muted session-info-truncate">
                  {agent.append_system_prompt.length > 80
                    ? agent.append_system_prompt.slice(0, 80) + "..."
                    : agent.append_system_prompt}
                </span>
              </Row>
            )}
          </div>
        </section>
      )}

      {/* ── Actions ── */}
      <section className="session-info-section">
        <h3 className="session-info-heading">Actions</h3>
        <div className="session-info-actions">
          <button
            className="session-info-btn session-info-btn-warning"
            onClick={handleKill}
            disabled={killing || !status?.process_alive}
            title={status?.process_alive ? "Force-kill the Claude process" : "No active process"}
          >
            {killing ? "Killing..." : "Kill Process"}
          </button>
          {!confirmDelete ? (
            <button
              className="session-info-btn session-info-btn-danger"
              onClick={() => setConfirmDelete(true)}
              disabled={deleting}
            >
              Delete Session
            </button>
          ) : (
            <div className="session-info-confirm">
              <span className="session-info-confirm-text">Delete this session?</span>
              <button
                className="session-info-btn session-info-btn-danger"
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? "Deleting..." : "Confirm"}
              </button>
              <button
                className="session-info-btn"
                onClick={() => setConfirmDelete(false)}
              >
                Cancel
              </button>
            </div>
          )}
        </div>
      </section>

      {error && <div className="session-info-error">{error}</div>}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="session-info-row">
      <span className="session-info-label">{label}</span>
      <span className="session-info-value">{children}</span>
    </div>
  );
}

function StatusDot({ alive, busy }: { alive: boolean; busy: boolean }) {
  const color = busy ? "var(--warning)" : alive ? "var(--success)" : "var(--text-secondary)";
  return (
    <span
      className="session-info-dot"
      style={{ background: color }}
    />
  );
}
