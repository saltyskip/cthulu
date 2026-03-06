import { useState, useRef, useCallback } from "react";
import AgentChatView, { useAgentChat } from "./AgentChatView";
import FileViewer from "./FileViewer";
import DebugPanel from "./DebugPanel";
import type { PendingPermission } from "../hooks/useGlobalPermissions";
import type { DebugEvent } from "./chat/useAgentChat";

export type ReferenceTab = "files" | "debug";

interface AgentDetailViewProps {
  agentId: string;
  agentName: string;
  sessionId: string;
  pendingPermissions: PendingPermission[];
  onPermissionResponse: (requestId: string, decision: "allow" | "deny") => void;
  hookDebugEvents: DebugEvent[];
  onClearHookDebug: () => void;
  onDeleted: () => void;
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
}: AgentDetailViewProps) {
  const chat = useAgentChat(agentId, sessionId);
  const [chatFlex, setChatFlex] = useState(1);
  const [filesFlex, setFilesFlex] = useState(1);
  const [referenceTab, setReferenceTab] = useState<ReferenceTab>("files");
  const containerRef = useRef<HTMLDivElement>(null);

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
        <AgentChatView
          chat={chat}
          pendingPermissions={pendingPermissions}
          onPermissionResponse={onPermissionResponse}
        />
      </div>
      <div className="agent-detail-divider" onMouseDown={handleDividerMouseDown} />
      <div className="agent-detail-files" style={{ flex: filesFlex }}>
        <div className="ref-content">
          {referenceTab === "files" ? (
            <FileViewer agentId={agentId} sessionId={sessionId} changedFiles={chat.changedFiles} />
          ) : (
            <DebugPanel
              chatEvents={chat.debugEvents}
              hookEvents={hookDebugEvents}
              onClearChat={chat.clearDebugEvents}
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
            className={`ref-toolbar-btn ${referenceTab === "debug" ? "ref-toolbar-btn-active" : ""}`}
            onClick={() => setReferenceTab("debug")}
            title="Debug"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M4.5 3A2.5 2.5 0 0 1 7 .5h2A2.5 2.5 0 0 1 11.5 3v.07a5 5 0 0 1 2.18 1.49l1.82-1.06.5.87-1.81 1.04A5 5 0 0 1 14.5 7H16v1h-1.5a5 5 0 0 1-.31 1.59l1.81 1.04-.5.87-1.82-1.06A5 5 0 0 1 11.5 12v1a2.5 2.5 0 0 1-2.5 2.5H7A2.5 2.5 0 0 1 4.5 13v-1a5 5 0 0 1-2.18-1.56L.5 11.5l-.5-.87 1.81-1.04A5 5 0 0 1 1.5 8H0V7h1.5a5 5 0 0 1 .31-1.59L.5 4.37l.5-.87 1.82 1.06A5 5 0 0 1 4.5 3.07V3zm1 0v.34l-.44.2A4 4 0 0 0 4 8a4 4 0 0 0 1.06 2.71l.44.42V13A1.5 1.5 0 0 0 7 14.5h2a1.5 1.5 0 0 0 1.5-1.5v-1.87l.44-.42A4 4 0 0 0 12 8a4 4 0 0 0-1.06-2.46l-.44-.2V3A1.5 1.5 0 0 0 9 1.5H7A1.5 1.5 0 0 0 5.5 3z"/>
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
