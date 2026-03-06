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

  const handleToggleDebug = useCallback(() => {
    setReferenceTab((prev) => (prev === "debug" ? "files" : "debug"));
  }, []);

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
          onToggleDebug={handleToggleDebug}
          debugActive={referenceTab === "debug"}
        />
      </div>
      <div className="agent-detail-divider" onMouseDown={handleDividerMouseDown} />
      <div className="agent-detail-files" style={{ flex: filesFlex }}>
        <div className="ref-tabs">
          <button
            className={`ref-tab ${referenceTab === "files" ? "ref-tab-active" : ""}`}
            onClick={() => setReferenceTab("files")}
          >
            Files
          </button>
          <button
            className={`ref-tab ${referenceTab === "debug" ? "ref-tab-active" : ""}`}
            onClick={() => setReferenceTab("debug")}
          >
            Debug
          </button>
        </div>
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
    </div>
  );
}
