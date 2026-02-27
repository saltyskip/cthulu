import { useState, useCallback, useRef } from "react";
import AgentEditor from "./AgentEditor";
import NodeTerminal from "./NodeTerminal";

interface AgentWorkspaceViewProps {
  agentId: string;
  agentName: string;
  onDeleted: () => void;
}

const MIN_EDITOR_WIDTH = 280;
const MAX_EDITOR_WIDTH = 600;

export default function AgentWorkspaceView({
  agentId,
  agentName,
  onDeleted,
}: AgentWorkspaceViewProps) {
  const [editorWidth, setEditorWidth] = useState(360);
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);

  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = { startX: e.clientX, startW: editorWidth };

      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        const delta = ev.clientX - dragRef.current.startX;
        const newW = Math.min(
          MAX_EDITOR_WIDTH,
          Math.max(MIN_EDITOR_WIDTH, dragRef.current.startW + delta)
        );
        setEditorWidth(newW);
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
    [editorWidth]
  );

  return (
    <div className="agent-workspace">
      <div className="agent-workspace-editor" style={{ width: editorWidth }}>
        <AgentEditor
          key={agentId}
          agentId={agentId}
          onClose={() => {}}
          onDeleted={onDeleted}
        />
      </div>
      <div
        className="agent-workspace-divider"
        onMouseDown={handleDragStart}
      />
      <div className="agent-workspace-terminal">
        <NodeTerminal
          key={`workspace:${agentId}`}
          agentId={agentId}
          nodeLabel={agentName}
          runtime="local"
        />
      </div>
    </div>
  );
}
