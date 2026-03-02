import type { InteractSessionInfo } from "../api/client";

interface SessionTabBarProps {
  sessions: InteractSessionInfo[];
  activeSessionId: string;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onDeleteSession: (id: string) => void;
  onKillSession?: (id: string) => void;
  interactiveCount?: number;
  maxSessions?: number;
}

export default function SessionTabBar({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
  onDeleteSession,
  onKillSession,
  interactiveCount = 0,
  maxSessions = 5,
}: SessionTabBarProps) {
  const atLimit = interactiveCount >= maxSessions;

  return (
    <div className="session-tabs">
      {sessions.map((s, i) => {
        const isFlowRun = s.kind === "flow_run";
        const label = isFlowRun
          ? s.flow_run?.flow_name || s.summary || `Flow Run ${i + 1}`
          : s.summary || `Session ${i + 1}`;

        // Tri-state: busy+alive = pulsing, idle+alive = solid, dead = gray
        const statusClass = s.busy
          ? "busy"
          : s.process_alive
            ? "alive"
            : "dead";

        // Show kill button when busy but process is dead (stuck state)
        const showKill = s.busy && !s.process_alive && onKillSession;

        return (
          <div
            key={s.session_id}
            className={`session-tab${s.session_id === activeSessionId ? " active" : ""}${isFlowRun ? " flow-run" : ""}`}
            onClick={() => onSelectSession(s.session_id)}
          >
            {isFlowRun && <span className="session-tab-flow-icon">&#9654;</span>}
            <span className="session-tab-status-dot" data-status={statusClass} />
            <span className="session-tab-label">{label}</span>
            {showKill && (
              <button
                className="session-tab-kill"
                onClick={(e) => {
                  e.stopPropagation();
                  onKillSession(s.session_id);
                }}
                title="Force kill stuck session"
              >
                &#8856;
              </button>
            )}
            {sessions.length > 1 && (
              <button
                className="session-tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  onDeleteSession(s.session_id);
                }}
                title="Close session"
              >
                &times;
              </button>
            )}
          </div>
        );
      })}
      <button
        className="session-tab-new"
        onClick={onNewSession}
        disabled={atLimit}
        title={atLimit ? `Session limit reached (${maxSessions} max)` : "New session"}
      >
        +
      </button>
      <span className="session-pool-count">
        {interactiveCount}/{maxSessions}
      </span>
    </div>
  );
}
