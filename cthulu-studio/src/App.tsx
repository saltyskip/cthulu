import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "./api/client";
import { log, getEntries, subscribe } from "./api/logger";
import type { Flow, FlowNode, FlowEdge, FlowSummary, NodeTypeSchema } from "./types/flow";
import TopBar from "./components/TopBar";
import FlowList from "./components/FlowList";
import Sidebar from "./components/Sidebar";
import Canvas from "./components/Canvas";
import PropertyPanel from "./components/PropertyPanel";
import RunHistory from "./components/RunHistory";
import Console from "./components/Console";

export default function App() {
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [activeFlow, setActiveFlow] = useState<Flow | null>(null);
  const [nodeTypes, setNodeTypes] = useState<NodeTypeSchema[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showConsole, setShowConsole] = useState(false);
  const [errorCount, setErrorCount] = useState(0);
  const [serverUrl, setServerUrlState] = useState(api.getServerUrl());
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Track error count for badge
  useEffect(() => {
    return subscribe(() => {
      const errors = getEntries().filter((e) => e.level === "error").length;
      setErrorCount(errors);
    });
  }, []);

  // Log startup
  useEffect(() => {
    log("info", "Cthulu Studio started");
    log("info", `Server URL: ${api.getServerUrl()}`);
    loadFlows();
    loadNodeTypes();
  }, []);

  const loadFlows = async () => {
    try {
      const data = await api.listFlows();
      setFlows(data);
    } catch {
      // Logged by API client
    }
  };

  const loadNodeTypes = async () => {
    try {
      const data = await api.getNodeTypes();
      setNodeTypes(data);
    } catch {
      // Logged by API client
    }
  };

  const selectFlow = async (id: string) => {
    try {
      const flow = await api.getFlow(id);
      setActiveFlow(flow);
      setSelectedNodeId(null);
    } catch {
      // Logged by API client
    }
  };

  const createFlow = async () => {
    try {
      const { id } = await api.createFlow("New Flow");
      await loadFlows();
      await selectFlow(id);
    } catch {
      // Logged by API client
    }
  };

  const debouncedSave = useCallback(
    (flow: Flow) => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(async () => {
        try {
          await api.updateFlow(flow.id, {
            name: flow.name,
            description: flow.description,
            enabled: flow.enabled,
            nodes: flow.nodes,
            edges: flow.edges,
          });
          loadFlows();
        } catch {
          // Logged by API client
        }
      }, 500);
    },
    []
  );

  const handleNodesChange = useCallback(
    (nodes: FlowNode[]) => {
      if (!activeFlow) return;
      const updated = { ...activeFlow, nodes };
      setActiveFlow(updated);
      debouncedSave(updated);
    },
    [activeFlow, debouncedSave]
  );

  const handleEdgesChange = useCallback(
    (edges: FlowEdge[]) => {
      if (!activeFlow) return;
      const updated = { ...activeFlow, edges };
      setActiveFlow(updated);
      debouncedSave(updated);
    },
    [activeFlow, debouncedSave]
  );

  const handleNodeSelect = useCallback((nodeId: string | null) => {
    setSelectedNodeId(nodeId);
  }, []);

  const handleNodeUpdate = useCallback(
    (nodeId: string, updates: Partial<FlowNode>) => {
      if (!activeFlow) return;
      const nodes = activeFlow.nodes.map((n) =>
        n.id === nodeId ? { ...n, ...updates } : n
      );
      const updated = { ...activeFlow, nodes };
      setActiveFlow(updated);
      debouncedSave(updated);
    },
    [activeFlow, debouncedSave]
  );

  const handleNodeDelete = useCallback(
    (nodeId: string) => {
      if (!activeFlow) return;
      const nodes = activeFlow.nodes.filter((n) => n.id !== nodeId);
      const edges = activeFlow.edges.filter(
        (e) => e.source !== nodeId && e.target !== nodeId
      );
      const updated = { ...activeFlow, nodes, edges };
      setActiveFlow(updated);
      setSelectedNodeId(null);
      debouncedSave(updated);
    },
    [activeFlow, debouncedSave]
  );

  const handleDrop = useCallback(
    (nodeType: NodeTypeSchema, position: { x: number; y: number }) => {
      if (!activeFlow) return;
      const newNode: FlowNode = {
        id: crypto.randomUUID(),
        node_type: nodeType.node_type,
        kind: nodeType.kind,
        config: {},
        position,
        label: nodeType.label,
      };
      const nodes = [...activeFlow.nodes, newNode];
      const updated = { ...activeFlow, nodes };
      setActiveFlow(updated);
      debouncedSave(updated);
      setSelectedNodeId(newNode.id);
    },
    [activeFlow, debouncedSave]
  );

  const handleDragStart = useCallback(
    (event: React.DragEvent, nodeType: NodeTypeSchema) => {
      event.dataTransfer.setData(
        "application/cthulu-node",
        JSON.stringify(nodeType)
      );
      event.dataTransfer.effectAllowed = "move";
    },
    []
  );

  const handleTrigger = async () => {
    if (!activeFlow) return;
    try {
      log("info", `Triggering flow: ${activeFlow.name}`);
      await api.triggerFlow(activeFlow.id);
    } catch {
      // Logged by API client
    }
  };

  const handleToggleEnabled = async () => {
    if (!activeFlow) return;
    const updated = { ...activeFlow, enabled: !activeFlow.enabled };
    setActiveFlow(updated);
    try {
      await api.updateFlow(activeFlow.id, { enabled: updated.enabled });
      loadFlows();
    } catch {
      // Logged by API client
    }
  };

  const handleDeleteFlow = async () => {
    if (!activeFlow) return;
    try {
      await api.deleteFlow(activeFlow.id);
      setActiveFlow(null);
      setSelectedNodeId(null);
      loadFlows();
    } catch {
      // Logged by API client
    }
  };

  const handleSaveSettings = () => {
    api.setServerUrl(serverUrl);
    setShowSettings(false);
    loadFlows();
    loadNodeTypes();
  };

  const selectedNode = activeFlow?.nodes.find((n) => n.id === selectedNodeId) || null;

  return (
    <div className={showConsole ? "app-with-console" : ""}>
      <TopBar
        flow={activeFlow}
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
            activeFlowId={activeFlow?.id || null}
            onSelect={selectFlow}
            onCreate={createFlow}
          />
          <Sidebar nodeTypes={nodeTypes} onDragStart={handleDragStart} />
        </div>

        {activeFlow ? (
          <Canvas
            flow={activeFlow}
            onNodesChange={handleNodesChange}
            onEdgesChange={handleEdgesChange}
            onNodeSelect={handleNodeSelect}
            onDrop={handleDrop}
          />
        ) : (
          <div className="canvas-container">
            <div className="empty-state">
              <p>Select a flow or create a new one</p>
            </div>
          </div>
        )}

        <div style={{ display: "flex", flexDirection: "column" }}>
          <PropertyPanel
            node={selectedNode}
            onUpdate={handleNodeUpdate}
            onDelete={handleNodeDelete}
          />
          <RunHistory flowId={activeFlow?.id || null} />
          {activeFlow && (
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
