import { useState, useCallback, useRef, useEffect, type RefObject } from "react";
import { STUDIO_ASSISTANT_ID, type Flow, type FlowNode, type FlowEdge, type RunEvent } from "../types/flow";
import { listAgentSessions, newAgentSession } from "../api/client";
import type { UpdateSignal } from "../hooks/useFlowDispatch";
import Canvas, { type CanvasHandle } from "./Canvas";
import FlowEditor, { type FlowEditorHandle } from "./FlowEditor";
import RunLog from "./RunLog";
import AgentChatView from "./AgentChatView";
import NodeConfigPanel from "./NodeConfigPanel";
import ErrorBoundary from "./ErrorBoundary";

interface FlowWorkspaceViewProps {
  flowId: string | null;
  canonicalFlow: Flow | null;
  updateSignal: UpdateSignal;
  canvasRef: RefObject<CanvasHandle | null>;
  onCanvasChange: (updates: { nodes: FlowNode[]; edges: FlowEdge[] }) => void;
  onEditorChange: (text: string) => void;
  onSelectionChange: (nodeId: string | null) => void;
  selectedNodeId: string | null;
  nodeRunStatus: Record<string, "running" | "completed" | "failed">;
  runEvents: RunEvent[];
  onRunEventsClear: () => void;
  runLogOpen: boolean;
  onRunLogClose: () => void;
}

const MIN_EDITOR_WIDTH = 280;
const MAX_EDITOR_WIDTH = 800;
const DEFAULT_EDITOR_WIDTH = 420;
const MIN_BOTTOM_HEIGHT = 120;
const MAX_BOTTOM_HEIGHT = 500;
const DEFAULT_BOTTOM_HEIGHT = 220;

type BottomTab = "log" | "terminal";

export default function FlowWorkspaceView({
  flowId,
  canonicalFlow,
  updateSignal,
  canvasRef,
  onCanvasChange,
  onEditorChange,
  onSelectionChange,
  selectedNodeId,
  nodeRunStatus,
  runEvents,
  onRunEventsClear,
  runLogOpen,
  onRunLogClose,
}: FlowWorkspaceViewProps) {
  const [editorWidth, setEditorWidth] = useState(DEFAULT_EDITOR_WIDTH);
  const [bottomHeight, setBottomHeight] = useState(DEFAULT_BOTTOM_HEIGHT);
  const [bottomOpen, setBottomOpen] = useState(false);
  const [bottomTab, setBottomTab] = useState<BottomTab>("log");

  const [studioSessionId, setStudioSessionId] = useState<string | null>(null);

  const editorRef = useRef<FlowEditorHandle>(null);
  const hDragRef = useRef<{ startX: number; startW: number } | null>(null);
  const vDragRef = useRef<{ startY: number; startH: number } | null>(null);

  // Monaco is uncontrolled — we push text imperatively via editorRef.setText().
  // This avoids the value-prop round-trip that causes cursor jumps.
  const lastEditorCounter = useRef(0);
  const editorDefaultText = useRef("");

  // Compute initial/switch text for defaultValue (only used at mount)
  const initialEditorText = canonicalFlow ? JSON.stringify(canonicalFlow, null, 2) : "";
  if (updateSignal.source === "init" && updateSignal.counter > 0) {
    editorDefaultText.current = initialEditorText;
  }

  // Auto-resolve or create a session for the Studio Assistant terminal
  useEffect(() => {
    if (bottomTab !== "terminal" || studioSessionId) return;
    let cancelled = false;
    (async () => {
      try {
        const info = await listAgentSessions(STUDIO_ASSISTANT_ID);
        if (cancelled) return;
        if (info.sessions.length > 0) {
          setStudioSessionId(info.active_session || info.sessions[0].session_id);
        } else {
          const result = await newAgentSession(STUDIO_ASSISTANT_ID);
          if (!cancelled) setStudioSessionId(result.session_id);
        }
      } catch {
        // server unreachable
      }
    })();
    return () => { cancelled = true; };
  }, [bottomTab, studioSessionId]);

  useEffect(() => {
    if (updateSignal.counter <= lastEditorCounter.current) return;
    lastEditorCounter.current = updateSignal.counter;
    // When the editor itself originated the change, don't touch Monaco
    if (updateSignal.source === "editor") return;
    const text = canonicalFlow ? JSON.stringify(canonicalFlow, null, 2) : "";
    editorRef.current?.setText(text);
  }, [updateSignal, canonicalFlow]);

  const handleLocalEditorChange = useCallback(
    (text: string) => {
      onEditorChange(text);
    },
    [onEditorChange]
  );

  // Open bottom pane when run log is requested
  useEffect(() => {
    if (runLogOpen && !bottomOpen) {
      setBottomOpen(true);
      setBottomTab("log");
    }
  }, [runLogOpen, bottomOpen]);

  // --- Click-to-jump: Canvas selection → Editor highlight ---
  const handleSelectionChange = useCallback(
    (nodeId: string | null) => {
      onSelectionChange(nodeId);
      if (nodeId) {
        editorRef.current?.revealNode(nodeId);
      }
    },
    [onSelectionChange]
  );

  // --- Horizontal divider drag (canvas | editor) ---
  const handleHDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      hDragRef.current = { startX: e.clientX, startW: editorWidth };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const onMove = (ev: MouseEvent) => {
        if (!hDragRef.current) return;
        // Dragging left increases editor width (editor is on the right)
        const delta = hDragRef.current.startX - ev.clientX;
        setEditorWidth(
          Math.min(MAX_EDITOR_WIDTH, Math.max(MIN_EDITOR_WIDTH, hDragRef.current.startW + delta))
        );
      };
      const onUp = () => {
        hDragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editorWidth]
  );

  // --- Vertical divider drag (main | bottom pane) ---
  const handleVDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      vDragRef.current = { startY: e.clientY, startH: bottomHeight };
      document.body.style.cursor = "row-resize";
      document.body.style.userSelect = "none";

      const onMove = (ev: MouseEvent) => {
        if (!vDragRef.current) return;
        const delta = vDragRef.current.startY - ev.clientY;
        setBottomHeight(
          Math.min(MAX_BOTTOM_HEIGHT, Math.max(MIN_BOTTOM_HEIGHT, vDragRef.current.startH + delta))
        );
      };
      const onUp = () => {
        vDragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [bottomHeight]
  );

  const handleBottomClose = useCallback(() => {
    setBottomOpen(false);
    onRunLogClose();
  }, [onRunLogClose]);

  return (
    <div className="flow-workspace">
      {/* Top area: Canvas + Editor */}
      <div className="flow-workspace-top">
        {flowId ? (
          <ErrorBoundary>
            <Canvas
              ref={canvasRef}
              flowId={flowId}
              canonicalFlow={canonicalFlow}
              updateSignal={updateSignal}
              onFlowChange={onCanvasChange}
              onSelectionChange={handleSelectionChange}
              nodeRunStatus={nodeRunStatus}
            />
          </ErrorBoundary>
        ) : (
          <div className="canvas-container">
            <div className="empty-state">
              <p>Select a flow or create a new one</p>
            </div>
          </div>
        )}

        {flowId && (
          <>
            <div
              className="flow-workspace-divider flow-workspace-divider-h"
              onMouseDown={handleHDragStart}
            />
            <div className="flow-workspace-editor" style={{ width: editorWidth }}>
              {selectedNodeId && canonicalFlow ? (
                <NodeConfigPanel
                  key={selectedNodeId}
                  nodeId={selectedNodeId}
                  canonicalFlow={canonicalFlow}
                  canvasRef={canvasRef}
                />
              ) : (
                <FlowEditor
                  key={flowId}
                  ref={editorRef}
                  defaultValue={editorDefaultText.current}
                  onChange={handleLocalEditorChange}
                />
              )}
            </div>
          </>
        )}
      </div>

      {/* Bottom pane */}
      {bottomOpen && (
        <>
          <div
            className="flow-workspace-divider flow-workspace-divider-v"
            onMouseDown={handleVDragStart}
          />
          <div className="flow-workspace-bottom" style={{ height: bottomHeight }}>
            <div className="flow-workspace-bottom-tabs">
              <button
                className={`flow-workspace-tab${bottomTab === "log" ? " active" : ""}`}
                onClick={() => setBottomTab("log")}
              >
                Run Log
              </button>
              <button
                className={`flow-workspace-tab${bottomTab === "terminal" ? " active" : ""}`}
                onClick={() => setBottomTab("terminal")}
              >
                Terminal
              </button>
              <div className="spacer" />
              <button className="flow-workspace-tab-close" onClick={handleBottomClose}>
                ×
              </button>
            </div>
            <div className="flow-workspace-bottom-content">
              {bottomTab === "log" && (
                <RunLog
                  events={runEvents}
                  onClear={onRunEventsClear}
                  onClose={handleBottomClose}
                />
              )}
              {bottomTab === "terminal" && studioSessionId && (
                <AgentChatView
                  key={`workspace-chat:${STUDIO_ASSISTANT_ID}`}
                  agentId={STUDIO_ASSISTANT_ID}
                  sessionId={studioSessionId}
                />
              )}
            </div>
          </div>
        </>
      )}

      {/* Toggle button when bottom is closed */}
      {!bottomOpen && flowId && (
        <div className="flow-workspace-bottom-toggle">
          <button
            className="flow-workspace-tab"
            onClick={() => { setBottomOpen(true); setBottomTab("log"); }}
          >
            Run Log
          </button>
          <button
            className="flow-workspace-tab"
            onClick={() => { setBottomOpen(true); setBottomTab("terminal"); }}
          >
            Terminal
          </button>
        </div>
      )}
    </div>
  );
}
