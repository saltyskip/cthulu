import type { InteractSessionInfo } from "../api/client";

interface SessionTabBarProps {
  sessions: InteractSessionInfo[];
  activeSessionId: string;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onDeleteSession: (id: string) => void;
}

export default function SessionTabBar({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
  onDeleteSession,
}: SessionTabBarProps) {
  return (
    <div className="session-tabs">
      {sessions.map((s, i) => {
        const isFlowRun = s.kind === "flow_run";
        const label = isFlowRun
          ? s.flow_run?.flow_name || s.summary || `Flow Run ${i + 1}`
          : s.summary || `Session ${i + 1}`;

        return (
          <div
            key={s.session_id}
            className={`session-tab${s.session_id === activeSessionId ? " active" : ""}${isFlowRun ? " flow-run" : ""}`}
            onClick={() => onSelectSession(s.session_id)}
          >
            {isFlowRun && <span className="session-tab-flow-icon">▶</span>}
            <span className="session-tab-label">{label}</span>
            {s.busy && <span className="session-tab-busy" />}
            {sessions.length > 1 && (
              <button
                className="session-tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  onDeleteSession(s.session_id);
                }}
                title="Close session"
              >
                ×
              </button>
            )}
          </div>
        );
      })}
      <button
        className="session-tab-new"
        onClick={onNewSession}
        title="New session"
      >
        +
      </button>
    </div>
  );
}
