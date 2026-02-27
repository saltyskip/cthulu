import { useState, useCallback, useRef, type RefObject } from "react";
import { STUDIO_ASSISTANT_ID, type Flow, type FlowNode, type FlowEdge, type RunEvent } from "../types/flow";
import Canvas, { type CanvasHandle } from "./Canvas";
import FlowEditor, { type FlowEditorHandle } from "./FlowEditor";
import RunLog from "./RunLog";
import NodeTerminal from "./NodeTerminal";
import ErrorBoundary from "./ErrorBoundary";

interface FlowWorkspaceViewProps {
  flowId: string | null;
  initialFlow: Flow | null;
  canvasRef: RefObject<CanvasHandle | null>;
  onFlowSnapshot: (snapshot: { nodes: FlowNode[]; edges: FlowEdge[] }) => void;
  onSelectionChange: (nodeId: string | null) => void;
  selectedNodeId: string | null;
  nodeRunStatus: Record<string, "running" | "completed" | "failed">;
  runEvents: RunEvent[];
  onRunEventsClear: () => void;
  runLogOpen: boolean;
  onRunLogClose: () => void;
  activeFlowMeta: { id: string; name: string; description: string; enabled: boolean } | null;
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
  initialFlow,
  canvasRef,
  onFlowSnapshot,
  onSelectionChange,
  selectedNodeId,
  nodeRunStatus,
  runEvents,
  onRunEventsClear,
  runLogOpen,
  onRunLogClose,
  activeFlowMeta,
}: FlowWorkspaceViewProps) {
  const [flowText, setFlowText] = useState("");
  const [editorWidth, setEditorWidth] = useState(DEFAULT_EDITOR_WIDTH);
  const [bottomHeight, setBottomHeight] = useState(DEFAULT_BOTTOM_HEIGHT);
  const [bottomOpen, setBottomOpen] = useState(false);
  const [bottomTab, setBottomTab] = useState<BottomTab>("log");

  const syncSource = useRef<"editor" | "canvas">("canvas");
  const editorRef = useRef<FlowEditorHandle>(null);
  const hDragRef = useRef<{ startX: number; startW: number } | null>(null);
  const vDragRef = useRef<{ startY: number; startH: number } | null>(null);

  // Track flow switches — seed editor text from initialFlow
  const prevFlowIdRef = useRef<string | null>(null);
  if (flowId !== prevFlowIdRef.current) {
    prevFlowIdRef.current = flowId;
    if (initialFlow && initialFlow.id === flowId) {
      const text = JSON.stringify(initialFlow, null, 2);
      setFlowText(text);
      syncSource.current = "canvas"; // initial load from flow data, treat as canvas source
    } else {
      setFlowText("");
    }
  }

  // Open bottom pane when run log is requested
  if (runLogOpen && !bottomOpen) {
    setBottomOpen(true);
    setBottomTab("log");
  }

  // --- Editor → Canvas sync ---
  const handleEditorChange = useCallback(
    (text: string) => {
      setFlowText(text);

      if (syncSource.current === "canvas") {
        syncSource.current = "editor";
        return;
      }

      syncSource.current = "editor";

      try {
        const parsed = JSON.parse(text) as Flow;
        if (!Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges)) return;
        canvasRef.current?.mergeFromFlow(parsed.nodes, parsed.edges);
      } catch {
        // Invalid JSON mid-edit — ignore
      }
    },
    [canvasRef]
  );

  // --- Canvas → Editor sync ---
  const handleCanvasSnapshot = useCallback(
    (snapshot: { nodes: FlowNode[]; edges: FlowEdge[] }) => {
      // Always forward to parent for API save + validation
      onFlowSnapshot(snapshot);

      if (syncSource.current === "editor") {
        syncSource.current = "canvas";
        return;
      }

      syncSource.current = "canvas";

      // Build full flow object for serialization
      if (!activeFlowMeta) return;
      const flow: Flow = {
        id: activeFlowMeta.id,
        name: activeFlowMeta.name,
        description: activeFlowMeta.description,
        enabled: activeFlowMeta.enabled,
        nodes: snapshot.nodes,
        edges: snapshot.edges,
        created_at: initialFlow?.created_at ?? "",
        updated_at: new Date().toISOString(),
      };
      setFlowText(JSON.stringify(flow, null, 2));
    },
    [onFlowSnapshot, activeFlowMeta, initialFlow]
  );

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
              initialFlow={initialFlow}
              onFlowSnapshot={handleCanvasSnapshot}
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
              <FlowEditor
                ref={editorRef}
                value={flowText}
                onChange={handleEditorChange}
              />
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
              {bottomTab === "terminal" && (
                <NodeTerminal
                  key={`workspace-term:${STUDIO_ASSISTANT_ID}`}
                  agentId={STUDIO_ASSISTANT_ID}
                  nodeLabel="Studio Assistant"
                  runtime="local"
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
