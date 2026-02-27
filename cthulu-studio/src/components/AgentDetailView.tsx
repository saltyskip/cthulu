import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import AgentEditor from "./AgentEditor";
import FlowRunChatView from "./FlowRunChatView";
import AgentChatView from "./AgentChatView";
import SessionTabBar from "./SessionTabBar";
import {
  listAgentSessions,
  newAgentSession,
  deleteAgentSession,
} from "../api/client";
import type { InteractSessionInfo } from "../api/client";

interface AgentDetailViewProps {
  agentId: string;
  agentName: string;
  onDeleted: () => void;
}

const MIN_CONFIG_WIDTH = 280;
const MAX_CONFIG_WIDTH = 500;

export default function AgentDetailView({
  agentId,
  agentName,
  onDeleted,
}: AgentDetailViewProps) {
  const [sessions, setSessions] = useState<InteractSessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string>("");
  const [mountedSessions, setMountedSessions] = useState<Set<string>>(new Set());
  const [configWidth, setConfigWidth] = useState(320);
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);

  // Fetch sessions on mount
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const info = await listAgentSessions(agentId);
        if (cancelled) return;
        if (info.sessions.length === 0) {
          // Auto-create first session
          const result = await newAgentSession(agentId);
          if (cancelled) return;
          setSessions([
            {
              session_id: result.session_id,
              summary: "",
              message_count: 0,
              total_cost: 0,
              created_at: result.created_at,
              busy: false,
              kind: "interactive",
            },
          ]);
          setActiveSessionId(result.session_id);
          setMountedSessions(new Set([result.session_id]));
        } else {
          setSessions(info.sessions);
          const active = info.active_session || info.sessions[0].session_id;
          setActiveSessionId(active);
          setMountedSessions(new Set([active]));
        }
      } catch {
        // server unreachable
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [agentId]);

  // Periodic refresh to detect new flow-run sessions
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const info = await listAgentSessions(agentId);
        setSessions((prev) => {
          // Only update if session count changed or busy state changed
          if (prev.length !== info.sessions.length) return info.sessions;
          const changed = prev.some((p) => {
            const match = info.sessions.find((s) => s.session_id === p.session_id);
            return match && (match.busy !== p.busy || match.kind !== p.kind);
          });
          return changed ? info.sessions : prev;
        });
      } catch {
        // ignore
      }
    }, 5000);
    return () => clearInterval(interval);
  }, [agentId]);

  const handleSelectSession = useCallback((sessionId: string) => {
    setActiveSessionId(sessionId);
    setMountedSessions((prev) => {
      const next = new Set(prev);
      next.add(sessionId);
      return next;
    });
  }, []);

  const handleNewSession = useCallback(async () => {
    try {
      const result = await newAgentSession(agentId);
      const newSession: InteractSessionInfo = {
        session_id: result.session_id,
        summary: "",
        message_count: 0,
        total_cost: 0,
        created_at: result.created_at,
        busy: false,
        kind: "interactive",
      };
      setSessions((prev) => [...prev, newSession]);
      setActiveSessionId(result.session_id);
      setMountedSessions((prev) => {
        const next = new Set(prev);
        next.add(result.session_id);
        return next;
      });
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  }, [agentId]);

  const handleDeleteSession = useCallback(
    async (sessionId: string) => {
      try {
        const result = await deleteAgentSession(agentId, sessionId);
        setSessions((prev) => prev.filter((s) => s.session_id !== sessionId));
        setMountedSessions((prev) => {
          const next = new Set(prev);
          next.delete(sessionId);
          return next;
        });
        if (activeSessionId === sessionId) {
          setActiveSessionId(result.active_session);
          setMountedSessions((prev) => {
            const next = new Set(prev);
            next.add(result.active_session);
            return next;
          });
        }
      } catch (e) {
        console.error("Failed to delete session:", e);
      }
    },
    [agentId, activeSessionId]
  );

  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = { startX: e.clientX, startW: configWidth };

      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        // Dragging left = making config wider (since config is on right)
        const delta = dragRef.current.startX - ev.clientX;
        const newW = Math.min(
          MAX_CONFIG_WIDTH,
          Math.max(MIN_CONFIG_WIDTH, dragRef.current.startW + delta)
        );
        setConfigWidth(newW);
      };

      const onUp = () => {
        dragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };

      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [configWidth]
  );

  return (
    <div className="agent-detail">
      <div className="agent-detail-main">
        <SessionTabBar
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
          onDeleteSession={handleDeleteSession}
        />
        <div className="agent-detail-terminals">
          {[...mountedSessions].map((sessionId) => {
            const session = sessions.find((s) => s.session_id === sessionId);
            const isFlowRun = session?.kind === "flow_run";

            return (
              <div
                key={sessionId}
                style={{
                  display: sessionId === activeSessionId ? "flex" : "none",
                  flex: 1,
                  flexDirection: "column",
                  minHeight: 0,
                }}
              >
                {isFlowRun ? (
                  <FlowRunChatView
                    agentId={agentId}
                    sessionId={sessionId}
                    busy={session?.busy ?? false}
                    flowRun={session?.flow_run}
                  />
                ) : (
                  <AgentChatView
                    agentId={agentId}
                    sessionId={sessionId}
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div
        className="agent-detail-divider"
        onMouseDown={handleDragStart}
      />
      <div className="agent-detail-config" style={{ width: configWidth }}>
        <AgentEditor
          key={agentId}
          agentId={agentId}
          onClose={() => {}}
          onDeleted={onDeleted}
        />
      </div>
    </div>
  );
}
