import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import * as api from "./api/client";
import { log } from "./api/logger";
import { subscribeToRuns } from "./api/runStream";
import type { Flow, FlowNode, FlowEdge, FlowSummary, NodeTypeSchema, RunEvent } from "./types/flow";
import TopBar from "./components/TopBar";
import Sidebar from "./components/Sidebar";
import FlowWorkspaceView from "./components/FlowWorkspaceView";
import AgentGridView from "./components/AgentGridView";
import AgentDetailView from "./components/AgentDetailView";
import PromptEditorView from "./components/PromptEditorView";
import { type CanvasHandle } from "./components/Canvas";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useFlowDispatch } from "./hooks/useFlowDispatch";

type ActiveView = "flow-editor" | "agent-grid" | "agent-workspace" | "prompt-editor";

export default function App() {
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [nodeTypes, setNodeTypes] = useState<NodeTypeSchema[]>([]);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [selectedAgentName, setSelectedAgentName] = useState<string | null>(null);
  const [visitedAgents, setVisitedAgents] = useState<Map<string, string>>(new Map()); // id -> name
  const [agentListKey, setAgentListKey] = useState(0);
  const [selectedPromptId, setSelectedPromptId] = useState<string | null>(null);
  const [promptListKey, setPromptListKey] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [runEvents, setRunEvents] = useState<RunEvent[]>([]);
  const [nodeRunStatus, setNodeRunStatus] = useState<Record<string, "running" | "completed" | "failed">>({});
  const [runLogOpen, setRunLogOpen] = useState(false);
  const [activeView, setActiveView] = useState<ActiveView>("flow-editor");

  // --- Central flow state (extracted hook) ---
  const activeFlowIdRef = useRef(activeFlowId);
  activeFlowIdRef.current = activeFlowId;

  const loadFlows = async () => {
    try { setFlows(await api.listFlows()); } catch { /* logged */ }
  };

  const dispatchApi = useMemo(() => ({
    onSaveComplete: loadFlows,
    updateFlow: api.updateFlow,
    getFlow: api.getFlow,
  }), []);

  const {
    canonicalFlow,
    updateSignal,
    flowVersionRef,
    dispatchFlowUpdate,
    initFlow,
  } = useFlowDispatch(dispatchApi, activeFlowIdRef);

  const activeFlowMeta = useMemo(() => {
    if (!canonicalFlow || !activeFlowId) return null;
    return { id: canonicalFlow.id, name: canonicalFlow.name, description: canonicalFlow.description, enabled: canonicalFlow.enabled };
  }, [canonicalFlow, activeFlowId]);

  const canvasRef = useRef<CanvasHandle>(null);

  // --- Server URL state ---
  const [serverUrl, setServerUrlState] = useState(api.getServerUrl());

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

      ghostEl.current?.remove();
      ghostEl.current = null;
      document.body.style.cursor = "";

      const el = document.elementFromPoint(e.clientX, e.clientY);
      if (!el?.closest(".canvas-container")) return;

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

  // --- SSE run event subscription ---
  const clearTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleRunEvent = useCallback((event: RunEvent) => {
    setRunEvents((prev) => {
      const next = [...prev, event];
      return next.length > 500 ? next.slice(-500) : next;
    });

    // Auto-open run log when a run starts
    if (event.event_type === "run_started") {
      if (clearTimer.current) clearTimeout(clearTimer.current);
      setNodeRunStatus({});
      setRunLogOpen(true);
    }

    if (event.node_id) {
      if (event.event_type === "node_started") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "running" }));
      } else if (event.event_type === "node_completed") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "completed" }));
      } else if (event.event_type === "node_failed") {
        setNodeRunStatus((prev) => ({ ...prev, [event.node_id!]: "failed" }));
      }
    }

    if (event.event_type === "run_completed" || event.event_type === "run_failed") {
      clearTimer.current = setTimeout(() => setNodeRunStatus({}), 10000);
    }
  }, []);

  useEffect(() => {
    if (!activeFlowId) return;
    setRunEvents([]);
    setNodeRunStatus({});
    const cleanup = subscribeToRuns(activeFlowId, handleRunEvent);
    return cleanup;
  }, [activeFlowId, handleRunEvent]);

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

  // --- File change subscription (SSE) ---
  useEffect(() => {
    const cleanup = api.subscribeToChanges((event) => {
      if (event.resource_type === "flow") {
        loadFlows();
        // If the active flow was updated externally, re-fetch and dispatch
        if (activeFlowIdRef.current && event.resource_id === activeFlowIdRef.current && event.change_type === "updated") {
          api.getFlow(activeFlowIdRef.current).then((flow) => {
            dispatchFlowUpdate("server", {
              nodes: flow.nodes,
              edges: flow.edges,
              name: flow.name,
              description: flow.description,
              enabled: flow.enabled,
              version: flow.version,
            });
          }).catch(() => { /* logged */ });
        }
      } else if (event.resource_type === "agent") {
        setAgentListKey((k) => k + 1);
      } else if (event.resource_type === "prompt") {
        setPromptListKey((k) => k + 1);
      }
    });
    return cleanup;
  }, [dispatchFlowUpdate]);

  const loadNodeTypes = async () => {
    try { setNodeTypes(await api.getNodeTypes()); } catch { /* logged */ }
  };

  const handleReconnect = useCallback(() => {
    loadFlows();
    loadNodeTypes();
  }, []);

  const selectFlow = async (id: string) => {
    try {
      const flow = await api.getFlow(id);
      setActiveFlowId(flow.id);
      initFlow(flow);
      setSelectedNodeId(null);
      setActiveView("flow-editor");
    } catch { /* logged */ }
  };

  const handleSelectAgent = async (agentId: string) => {
    try {
      const agent = await api.getAgent(agentId);
      setSelectedAgentId(agentId);
      setSelectedAgentName(agent.name);
      setVisitedAgents((prev) => new Map(prev).set(agentId, agent.name));
      setSelectedNodeId(null);
      setActiveView("agent-workspace");
    } catch { /* logged */ }
  };

  const handleBackToFlow = () => {
    setActiveView("flow-editor");
  };

  const handleShowAgentGrid = () => {
    setSelectedAgentId(null);
    setSelectedAgentName(null);
    setActiveView("agent-grid");
  };

  const handleSelectPrompt = (promptId: string) => {
    setSelectedPromptId(promptId);
    setActiveView("prompt-editor");
  };

  const createFlow = async () => {
    try {
      const { id } = await api.createFlow("New Flow");
      await loadFlows();
      await selectFlow(id);
    } catch { /* logged */ }
  };

  const handleImportTemplate = async (flow: Flow) => {
    try {
      await loadFlows();
      await selectFlow(flow.id);
    } catch { /* logged */ }
  };

  // --- Canvas change callback ---
  const handleCanvasChange = useCallback((updates: { nodes: FlowNode[]; edges: FlowEdge[] }) => {
    dispatchFlowUpdate("canvas", updates);
  }, [dispatchFlowUpdate]);

  // --- Editor change callback ---
  const handleEditorChange = useCallback((text: string) => {
    try {
      const parsed = JSON.parse(text) as Flow;
      if (!Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges)) return;

      // Strip version — version is server-controlled only
      dispatchFlowUpdate("editor", {
        nodes: parsed.nodes,
        edges: parsed.edges,
        name: parsed.name,
        description: parsed.description,
      });
    } catch {
      // Invalid JSON mid-edit — ignore
    }
  }, [dispatchFlowUpdate]);

  const handleSelectionChange = useCallback((nodeId: string | null) => {
    setSelectedNodeId(nodeId);
  }, []);

  const handleRename = async (name: string) => {
    if (!activeFlowMeta || !activeFlowId) return;
    dispatchFlowUpdate("app", { name });
  };

  const handleTrigger = async () => {
    if (!activeFlowMeta) return;
    try {
      log("info", `Triggering flow: ${activeFlowMeta.name}`);
      setRunLogOpen(true);
      await api.triggerFlow(activeFlowMeta.id);
    } catch { /* logged */ }
  };

  const handleToggleFlowEnabled = async (flowId: string) => {
    const flow = flows.find((f) => f.id === flowId);
    if (!flow) return;
    const newEnabled = !flow.enabled;
    setFlows((prev) =>
      prev.map((f) => (f.id === flowId ? { ...f, enabled: newEnabled } : f))
    );
    if (activeFlowMeta && activeFlowMeta.id === flowId) {
      dispatchFlowUpdate("app", { enabled: newEnabled });
    }
    try {
      await api.updateFlow(flowId, { enabled: newEnabled });
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
    <div className="app">
      <TopBar
        activeView={activeView}
        flow={activeFlowMeta}
        flowId={activeFlowId}
        onTrigger={handleTrigger}
        onRename={handleRename}
        agentName={selectedAgentName}
        onBackToFlow={handleBackToFlow}
        onShowAgentGrid={handleShowAgentGrid}
        onSettingsClick={() => setShowSettings(true)}
        onReconnect={handleReconnect}
      />
      <div className="app-layout">
        <Sidebar
          flows={flows}
          activeFlowId={activeFlowId}
          onSelectFlow={selectFlow}
          onCreateFlow={createFlow}
          onImportTemplate={handleImportTemplate}
          onToggleEnabled={handleToggleFlowEnabled}
          selectedAgentId={selectedAgentId}
          onSelectAgent={handleSelectAgent}
          onShowAgentGrid={handleShowAgentGrid}
          agentListKey={agentListKey}
          onAgentCreated={(id) => {
            handleSelectAgent(id);
          }}
          selectedPromptId={selectedPromptId}
          onSelectPrompt={handleSelectPrompt}
          promptListKey={promptListKey}
          activeView={activeView}
          nodeTypes={nodeTypes}
          onGrab={handleGrab}
        />

        <div style={{ display: activeView === "flow-editor" ? "contents" : "none" }}>
          <FlowWorkspaceView
            flowId={activeFlowId}
            canonicalFlow={canonicalFlow}
            updateSignal={updateSignal}
            canvasRef={canvasRef}
            onCanvasChange={handleCanvasChange}
            onEditorChange={handleEditorChange}
            onSelectionChange={handleSelectionChange}
            selectedNodeId={selectedNodeId}
            nodeRunStatus={nodeRunStatus}
            runEvents={runEvents}
            onRunEventsClear={() => setRunEvents([])}
            runLogOpen={runLogOpen}
            onRunLogClose={() => setRunLogOpen(false)}
          />
        </div>
        {activeView === "agent-grid" && (
          <AgentGridView
            onSelectAgent={handleSelectAgent}
            onCreateAgent={async () => {
              try {
                const { id } = await api.createAgent({ name: "New Agent" });
                setAgentListKey((k) => k + 1);
                handleSelectAgent(id);
              } catch { /* logged */ }
            }}
            agentListKey={agentListKey}
          />
        )}
        {activeView === "prompt-editor" && selectedPromptId && (
          <PromptEditorView
            key={selectedPromptId}
            promptId={selectedPromptId}
            onDeleted={() => {
              setSelectedPromptId(null);
              setPromptListKey((k) => k + 1);
              setActiveView("flow-editor");
            }}
            onBack={handleBackToFlow}
            onTitleChanged={() => {
              setPromptListKey((k) => k + 1);
            }}
          />
        )}
        {[...visitedAgents.entries()].map(([agentId, agentName]) => (
          <div
            key={agentId}
            style={{ display: activeView === "agent-workspace" && selectedAgentId === agentId ? "contents" : "none" }}
          >
            <AgentDetailView
              agentId={agentId}
              agentName={agentName}
              onDeleted={() => {
                setVisitedAgents((prev) => { const next = new Map(prev); next.delete(agentId); return next; });
                setSelectedAgentId(null);
                setSelectedAgentName(null);
                setAgentListKey((k) => k + 1);
                setActiveView("agent-grid");
              }}
            />
          </div>
        ))}
      </div>

      <Dialog open={showSettings} onOpenChange={setShowSettings}>
        <DialogContent className="bg-[var(--bg-secondary)] border-[var(--border)] text-[var(--text)]">
          <DialogHeader>
            <DialogTitle>Server Settings</DialogTitle>
          </DialogHeader>
          <div className="form-group">
            <label>Server URL</label>
            <input
              value={serverUrl}
              onChange={(e) => setServerUrlState(e.target.value)}
              placeholder="http://localhost:8081"
              className="w-full bg-[var(--bg)] border border-[var(--border)] rounded-md px-3 py-2 text-[var(--text)] text-sm outline-none focus:border-[var(--accent)]"
            />
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setShowSettings(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveSettings}>
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
