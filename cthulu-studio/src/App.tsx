import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import * as api from "./api/client";
import { log } from "./api/logger";
import { subscribeToRuns } from "./api/runStream";
import type { Flow, FlowNode, FlowEdge, FlowSummary, NodeTypeSchema, RunEvent, ActiveView } from "./types/flow";
import TopBar from "./components/TopBar";
import Sidebar from "./components/Sidebar";
import FlowWorkspaceView from "./components/FlowWorkspaceView";
import AgentDetailView from "./components/AgentDetailView";
import PromptEditorView from "./components/PromptEditorView";
import DashboardView from "./components/DashboardView";
import { useGlobalPermissions } from "./hooks/useGlobalPermissions";
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

export default function App() {
  const globalPermissions = useGlobalPermissions();
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [nodeTypes, setNodeTypes] = useState<NodeTypeSchema[]>([]);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedAgentName, setSelectedAgentName] = useState<string | null>(null);
  const [visitedAgents, setVisitedAgents] = useState<Map<string, { name: string; sessionId: string }>>(new Map());
  const [agentListKey, setAgentListKey] = useState(0);
  const [selectedPromptId, setSelectedPromptId] = useState<string | null>(null);
  const [promptListKey, setPromptListKey] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [runEvents, setRunEvents] = useState<RunEvent[]>([]);
  const [nodeRunStatus, setNodeRunStatus] = useState<Record<string, "running" | "completed" | "failed">>({});
  const [runLogOpen, setRunLogOpen] = useState(false);
  const [activeView, setActiveView] = useState<ActiveView>("flow-editor");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

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

  const handleSelectSession = useCallback(async (agentId: string, sessionId: string) => {
    try {
      const agent = await api.getAgent(agentId);
      setSelectedAgentId(agentId);
      setSelectedSessionId(sessionId);
      setSelectedAgentName(agent.name);
      setVisitedAgents((prev) => new Map(prev).set(agentId, { name: agent.name, sessionId }));
      setSelectedNodeId(null);
      setActiveView("agent-workspace");
    } catch { /* logged */ }
  }, []);

  const handleBackToFlow = () => {
    setActiveView("flow-editor");
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
        onSettingsClick={() => setShowSettings(true)}
        onReconnect={handleReconnect}
      />
      <div className="app-layout">
        {sidebarCollapsed ? (
          <div className="sidebar-collapsed" onClick={() => setSidebarCollapsed(false)}>
            <span className="sidebar-collapsed-icon">◧</span>
            <span className="sidebar-collapsed-label">Nav</span>
          </div>
        ) : (
          <Sidebar
            flows={flows}
            activeFlowId={activeFlowId}
            onSelectFlow={selectFlow}
            onCreateFlow={createFlow}
            onImportTemplate={handleImportTemplate}
            onToggleEnabled={handleToggleFlowEnabled}
            selectedAgentId={selectedAgentId}
            selectedSessionId={selectedSessionId}
            onSelectSession={handleSelectSession}
            agentListKey={agentListKey}
            onAgentCreated={(id) => {
              // When a new agent is created, create its first session and select it
              (async () => {
                try {
                  const result = await api.newAgentSession(id);
                  handleSelectSession(id, result.session_id);
                } catch {
                  // Fall back to just selecting the agent without a session
                }
              })();
            }}
            selectedPromptId={selectedPromptId}
            onSelectPrompt={handleSelectPrompt}
            promptListKey={promptListKey}
            activeView={activeView}
            nodeTypes={nodeTypes}
            onGrab={handleGrab}
            onCollapse={() => setSidebarCollapsed(true)}
            onSelectDashboard={() => setActiveView("dashboard")}
          />
        )}

        {activeView === "dashboard" && <DashboardView />}

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

        {[...visitedAgents.entries()].map(([agentId, { name: agentName, sessionId }]) => (
          <div
            key={`${agentId}::${sessionId}`}
            style={{ display: activeView === "agent-workspace" && selectedAgentId === agentId && selectedSessionId === sessionId ? "contents" : "none" }}
          >
            <AgentDetailView
              agentId={agentId}
              agentName={agentName}
              sessionId={sessionId}
              pendingPermissions={globalPermissions.permissionsForSession(agentId, sessionId)}
              onPermissionResponse={globalPermissions.respondToPermission}
              hookDebugEvents={globalPermissions.hookDebugEvents}
              onClearHookDebug={globalPermissions.clearHookDebugEvents}
              fileChanges={globalPermissions.fileChanges}
              onDeleted={() => {
                setVisitedAgents((prev) => { const next = new Map(prev); next.delete(agentId); return next; });
                setSelectedAgentId(null);
                setSelectedSessionId(null);
                setSelectedAgentName(null);
                setAgentListKey((k) => k + 1);
                setActiveView("flow-editor");
              }}
            />
          </div>
        ))}
      </div>

      <Dialog open={showSettings} onOpenChange={setShowSettings}>
        <DialogContent className="cth-dialog">
          <DialogHeader>
            <DialogTitle>Server Settings</DialogTitle>
          </DialogHeader>
          <div className="cth-dialog-field">
            <label className="cth-dialog-label">Server URL</label>
            <input
              value={serverUrl}
              onChange={(e) => setServerUrlState(e.target.value)}
              placeholder="http://localhost:8081"
              className="cth-dialog-input"
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
