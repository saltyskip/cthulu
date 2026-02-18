import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "./api/client";
import { log, getEntries, subscribe } from "./api/logger";
import type { Flow, FlowNode, FlowEdge, FlowSummary, NodeTypeSchema } from "./types/flow";
import TopBar from "./components/TopBar";
import FlowList from "./components/FlowList";
import Sidebar from "./components/Sidebar";
import Canvas, { type CanvasHandle } from "./components/Canvas";
import PropertyPanel from "./components/PropertyPanel";
import RunHistory from "./components/RunHistory";
import Console from "./components/Console";
import ErrorBoundary from "./components/ErrorBoundary";

export default function App() {
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [initialFlow, setInitialFlow] = useState<Flow | null>(null);
  const [nodeTypes, setNodeTypes] = useState<NodeTypeSchema[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showConsole, setShowConsole] = useState(false);
  const [errorCount, setErrorCount] = useState(0);
  const [serverUrl, setServerUrlState] = useState(api.getServerUrl());
  // Keep a light reference for TopBar (name, enabled) without driving Canvas
  const [activeFlowMeta, setActiveFlowMeta] = useState<{ id: string; name: string; description: string; enabled: boolean } | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const canvasRef = useRef<CanvasHandle>(null);

  // --- Drag-and-drop state (refs to avoid re-renders during drag) ---
  const dragging = useRef<NodeTypeSchema | null>(null);
  const ghostEl = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!ghostEl.current) return;
      ghostEl.current.style.left = `${e.clientX + 12}px`;
      ghostEl.current.style.top = `${e.clientY + 12}px`;
    };

    const onMouseUp = (e: MouseEvent) => {
      const nt = dragging.current;
      if (!nt) return;
      dragging.current = null;

      // Clean up ghost
      ghostEl.current?.remove();
      ghostEl.current = null;
      document.body.style.cursor = "";

      // Hit-test: is the cursor over the canvas?
      const el = document.elementFromPoint(e.clientX, e.clientY);
      if (!el?.closest(".canvas-container")) return;

      // Place the node — Canvas owns state, no need to update App
      canvasRef.current?.addNodeAtScreen(
        nt.node_type, nt.kind, nt.label, e.clientX, e.clientY
      );
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  const handleGrab = useCallback((nodeType: NodeTypeSchema) => {
    dragging.current = nodeType;
    document.body.style.cursor = "grabbing";

    const ghost = document.createElement("div");
    ghost.className = "drag-ghost";
    ghost.textContent = nodeType.label;
    document.body.appendChild(ghost);
    ghostEl.current = ghost;
  }, []);

  // --- Error count badge ---
  useEffect(() => {
    return subscribe(() => {
      const errors = getEntries().filter((e) => e.level === "error").length;
      setErrorCount(errors);
    });
  }, []);

  // --- Boot ---
  const initialized = useRef(false);
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    log("info", "Cthulu Studio started");
    log("info", `Server URL: ${api.getServerUrl()}`);
    loadFlows();
    loadNodeTypes();
  }, []);

  // --- API helpers ---
  const loadFlows = async () => {
    try { setFlows(await api.listFlows()); } catch { /* logged */ }
  };

  const loadNodeTypes = async () => {
    try { setNodeTypes(await api.getNodeTypes()); } catch { /* logged */ }
  };

  const selectFlow = async (id: string) => {
    try {
      const flow = await api.getFlow(id);
      setInitialFlow(flow);
      setActiveFlowId(flow.id);
      setActiveFlowMeta({ id: flow.id, name: flow.name, description: flow.description, enabled: flow.enabled });
      setSelectedNodeId(null);
    } catch { /* logged */ }
  };

  const createFlow = async () => {
    try {
      const { id } = await api.createFlow("New Flow");
      await loadFlows();
      await selectFlow(id);
    } catch { /* logged */ }
  };

  // --- Snapshot callback: Canvas pushes state here for persistence ---
  const handleFlowSnapshot = useCallback((snapshot: { nodes: FlowNode[]; edges: FlowEdge[] }) => {
    if (!activeFlowId || !activeFlowMeta) return;
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(async () => {
      try {
        await api.updateFlow(activeFlowId, {
          name: activeFlowMeta.name,
          description: activeFlowMeta.description,
          enabled: activeFlowMeta.enabled,
          nodes: snapshot.nodes,
          edges: snapshot.edges,
        });
        loadFlows();
      } catch { /* logged */ }
    }, 500);
  }, [activeFlowId, activeFlowMeta]);

  const handleSelectionChange = useCallback((nodeId: string | null) => {
    setSelectedNodeId(nodeId);
  }, []);

  const handleTrigger = async () => {
    if (!activeFlowMeta) return;
    try {
      log("info", `Triggering flow: ${activeFlowMeta.name}`);
      await api.triggerFlow(activeFlowMeta.id);
    } catch { /* logged */ }
  };

  const handleToggleEnabled = async () => {
    if (!activeFlowMeta) return;
    const updated = { ...activeFlowMeta, enabled: !activeFlowMeta.enabled };
    setActiveFlowMeta(updated);
    try {
      await api.updateFlow(activeFlowMeta.id, { enabled: updated.enabled });
      loadFlows();
    } catch { /* logged */ }
  };

  const handleDeleteFlow = async () => {
    if (!activeFlowId) return;
    try {
      await api.deleteFlow(activeFlowId);
      setActiveFlowId(null);
      setInitialFlow(null);
      setActiveFlowMeta(null);
      setSelectedNodeId(null);
      loadFlows();
    } catch { /* logged */ }
  };

  const handleSaveSettings = () => {
    api.setServerUrl(serverUrl);
    setShowSettings(false);
    loadFlows();
    loadNodeTypes();
  };

  return (
    <div className={showConsole ? "app-with-console" : ""}>
      <TopBar
        flow={activeFlowMeta}
        onTrigger={handleTrigger}
        onToggleEnabled={handleToggleEnabled}
        onSettingsClick={() => setShowSettings(true)}
        consoleOpen={showConsole}
        onToggleConsole={() => setShowConsole((v) => !v)}
        errorCount={errorCount}
      />
      <div className="app-layout">
        <div style={{ display: "flex", flexDirection: "column" }}>
          <FlowList
            flows={flows}
            activeFlowId={activeFlowId}
            onSelect={selectFlow}
            onCreate={createFlow}
          />
          <Sidebar nodeTypes={nodeTypes} onGrab={handleGrab} />
        </div>

        {activeFlowId ? (
          <ErrorBoundary>
            <Canvas
              ref={canvasRef}
              flowId={activeFlowId}
              initialFlow={initialFlow}
              onFlowSnapshot={handleFlowSnapshot}
              onSelectionChange={handleSelectionChange}
            />
          </ErrorBoundary>
        ) : (
          <div className="canvas-container">
            <div className="empty-state">
              <p>Select a flow or create a new one</p>
            </div>
          </div>
        )}

        <div style={{ display: "flex", flexDirection: "column" }}>
          <PropertyPanel
            canvasRef={canvasRef}
            selectedNodeId={selectedNodeId}
          />
          <RunHistory flowId={activeFlowId} />
          {activeFlowId && (
            <div style={{ padding: 16 }}>
              <button className="danger" onClick={handleDeleteFlow}>
                Delete Flow
              </button>
            </div>
          )}
        </div>
      </div>

      {showConsole && <Console onClose={() => setShowConsole(false)} />}

      {showSettings && (
        <div className="modal-overlay" onClick={() => setShowSettings(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Server Settings</h2>
            <div className="form-group">
              <label>Server URL</label>
              <input
                value={serverUrl}
                onChange={(e) => setServerUrlState(e.target.value)}
                placeholder="http://localhost:8081"
              />
            </div>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setShowSettings(false)}>
                Cancel
              </button>
              <button className="primary" onClick={handleSaveSettings}>
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
