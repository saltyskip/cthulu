import { useState, useRef, useCallback } from "react";
import AgentChatView, { useAgentChat } from "./AgentChatView";
import FileViewer from "./FileViewer";

interface AgentDetailViewProps {
  agentId: string;
  agentName: string;
  sessionId: string;
  onDeleted: () => void;
}

const MIN_CHAT_WIDTH = 320;
const MIN_FILES_WIDTH = 280;

export default function AgentDetailView({
  agentId,
  agentName: _agentName,
  sessionId,
  onDeleted: _onDeleted,
}: AgentDetailViewProps) {
  const chat = useAgentChat(agentId, sessionId);
  const [chatFlex, setChatFlex] = useState(1);
  const [filesFlex, setFilesFlex] = useState(1);
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
        // enforce minimums
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
        <AgentChatView chat={chat} />
      </div>
      <div className="agent-detail-divider" onMouseDown={handleDividerMouseDown} />
      <div className="agent-detail-files" style={{ flex: filesFlex }}>
        <FileViewer agentId={agentId} sessionId={sessionId} changedFiles={chat.changedFiles} />
      </div>
    </div>
  );
}
