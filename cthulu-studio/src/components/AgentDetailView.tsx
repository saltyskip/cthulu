import { useState, useRef, useCallback, useMemo } from "react";
import AgentTerminal from "./AgentTerminal";
import FileViewer from "./FileViewer";
import DebugPanel from "./DebugPanel";
import ChangesPanel from "./ChangesPanel";
import HeartbeatRunsPanel from "./HeartbeatRunsPanel";
import type { PendingPermission, FileChangeData } from "../hooks/useGlobalPermissions";
import type { DebugEvent } from "./chat/useAgentChat";

export type ReferenceTab = "files" | "changes" | "debug" | "heartbeat";

interface AgentDetailViewProps {
  agentId: string;
  agentName: string;
  sessionId: string;
  pendingPermissions: PendingPermission[];
  onPermissionResponse: (requestId: string, decision: "allow" | "deny") => void;
  hookDebugEvents: DebugEvent[];
  onClearHookDebug: () => void;
  onDeleted: () => void;
  fileChanges: FileChangeData[];
}

const MIN_CHAT_WIDTH = 320;
const MIN_FILES_WIDTH = 280;

export default function AgentDetailView({
  agentId,
  agentName: _agentName,
  sessionId,
  pendingPermissions,
  onPermissionResponse,
  hookDebugEvents,
  onClearHookDebug,
  onDeleted: _onDeleted,
  fileChanges,
}: AgentDetailViewProps) {
  const [chatFlex, setChatFlex] = useState(1);
  const [filesFlex, setFilesFlex] = useState(1);
  const [referenceTab, setReferenceTab] = useState<ReferenceTab>("files");
  const containerRef = useRef<HTMLDivElement>(null);

  // Derive hookChangedFiles from fileChanges filtered by this agent/session
  const hookChangedFiles = useMemo(() => {
    const paths = new Set<string>();
    for (const fc of fileChanges) {
      if (fc.agent_id === agentId && fc.session_id === sessionId) {
        const input = fc.tool_input;
        const filePath = (input.file_path ?? input.path ?? input.filename) as string | undefined;
        if (filePath) paths.add(filePath);
      }
    }
    return Array.from(paths);
  }, [fileChanges, agentId, sessionId]);

  const handleDividerMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const container = containerRef.current;
      if (!container) return;
      const startX = e.clientX;
      const totalWidth = container.getBoundingClientRect().width;
      const startChatFrac = chatFlex / (chatFlex + filesFlex);

      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        let newChatFrac = startChatFrac + dx / totalWidth;
        const minChatFrac = MIN_CHAT_WIDTH / totalWidth;
        const maxChatFrac = 1 - MIN_FILES_WIDTH / totalWidth;
        newChatFrac = Math.max(minChatFrac, Math.min(maxChatFrac, newChatFrac));
        setChatFlex(newChatFrac);
        setFilesFlex(1 - newChatFrac);
      };
      const onUp = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [chatFlex, filesFlex]
  );

  return (
    <div className="agent-detail" ref={containerRef}>
      <div className="agent-detail-chat" style={{ flex: chatFlex }}>
        <AgentTerminal agentId={agentId} sessionId={sessionId} />
      </div>
      <div className="agent-detail-divider" onMouseDown={handleDividerMouseDown} />
      <div className="agent-detail-files" style={{ flex: filesFlex }}>
        <div className="ref-content">
          {referenceTab === "files" ? (
            <FileViewer agentId={agentId} sessionId={sessionId} changedFiles={hookChangedFiles} />
          ) : referenceTab === "changes" ? (
            <ChangesPanel
              agentId={agentId}
              sessionId={sessionId}
              gitSnapshot={null}
              hookChangedFiles={hookChangedFiles}
            />
          ) : referenceTab === "heartbeat" ? (
            <HeartbeatRunsPanel agentId={agentId} />
          ) : (
            <DebugPanel
              chatEvents={[]}
              hookEvents={hookDebugEvents}
              onClearChat={() => {}}
              onClearHook={onClearHookDebug}
            />
          )}
        </div>
        <div className="ref-toolbar">
          <button
            className={`ref-toolbar-btn ${referenceTab === "files" ? "ref-toolbar-btn-active" : ""}`}
            onClick={() => setReferenceTab("files")}
            title="Files"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1.5 1h5l1 1H14.5a.5.5 0 0 1 .5.5v11a.5.5 0 0 1-.5.5h-13a.5.5 0 0 1-.5-.5v-12A.5.5 0 0 1 1.5 1zm0 1v11h13V3H7.25l-1-1H1.5z"/>
            </svg>
          </button>
          <button
            className={`ref-toolbar-btn ${referenceTab === "changes" ? "ref-toolbar-btn-active" : ""}`}
            onClick={() => setReferenceTab("changes")}
            title="Changes"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6A1.5 1.5 0 0 0 4.5 10v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.99 2.99 0 0 1 6 7h4a1.5 1.5 0 0 0 1.5-1.5v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0z"/>
            </svg>
          </button>
          <button
            className={`ref-toolbar-btn ${referenceTab === "debug" ? "ref-toolbar-btn-active" : ""}`}
            onClick={() => setReferenceTab("debug")}
            title="Debug"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M4.5 3A2.5 2.5 0 0 1 7 .5h2A2.5 2.5 0 0 1 11.5 3v.07a5 5 0 0 1 2.18 1.49l1.82-1.06.5.87-1.81 1.04A5 5 0 0 1 14.5 7H16v1h-1.5a5 5 0 0 1-.31 1.59l1.81 1.04-.5.87-1.82-1.06A5 5 0 0 1 11.5 12v1a2.5 2.5 0 0 1-2.5 2.5H7A2.5 2.5 0 0 1 4.5 13v-1a5 5 0 0 1-2.18-1.56L.5 11.5l-.5-.87 1.81-1.04A5 5 0 0 1 1.5 8H0V7h1.5a5 5 0 0 1 .31-1.59L.5 4.37l.5-.87 1.82 1.06A5 5 0 0 1 4.5 3.07V3zm1 0v.34l-.44.2A4 4 0 0 0 4 8a4 4 0 0 0 1.06 2.71l.44.42V13A1.5 1.5 0 0 0 7 14.5h2a1.5 1.5 0 0 0 1.5-1.5v-1.87l.44-.42A4 4 0 0 0 12 8a4 4 0 0 0-1.06-2.46l-.44-.2V3A1.5 1.5 0 0 0 9 1.5H7A1.5 1.5 0 0 0 5.5 3z"/>
            </svg>
          </button>
          <button
            className={`ref-toolbar-btn ${referenceTab === "heartbeat" ? "ref-toolbar-btn-active" : ""}`}
            onClick={() => setReferenceTab("heartbeat")}
            title="Heartbeat"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 2.748l-.717-.737C5.6.281 2.514.878 1.4 3.053c-.523 1.023-.641 2.5.314 4.385.92 1.815 2.834 3.989 6.286 6.357 3.452-2.368 5.365-4.542 6.286-6.357.955-1.886.837-3.362.314-4.385C13.486.878 10.4.28 8.717 2.01L8 2.748zM8 15C-7.333 4.868 3.279-3.04 7.824 1.143c.06.055.119.112.176.171a3.12 3.12 0 0 1 .176-.17C12.72-3.042 23.333 4.867 8 15z"/>
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
