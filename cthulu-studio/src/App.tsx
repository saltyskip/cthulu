import { useEffect, useCallback, useRef, useMemo } from "react";
import * as api from "./api/client";
import { log } from "./api/logger";
import { subscribeToRuns } from "./api/runStream";
import type { Flow, FlowNode, FlowEdge, NodeTypeSchema } from "./types/flow";
import TopBar from "./components/TopBar";
import Sidebar from "./components/Sidebar";
import FlowWorkspaceView from "./components/FlowWorkspaceView";
import AgentDetailView from "./components/AgentDetailView";
import PromptEditorView from "./components/PromptEditorView";
import DashboardView from "./components/DashboardView";
import { useGlobalPermissions } from "./hooks/useGlobalPermissions";
import AuthGate from "./components/AuthGate";
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
import { useFlowStore } from "./stores/useFlowStore";
import { useAgentStore } from "./stores/useAgentStore";
import { useViewStore } from "./stores/useViewStore";
import { useState } from "react";

export default function App() {
  const globalPermissions = useGlobalPermissions();

  // --- Zustand stores ---
  const flows = useFlowStore((s) => s.flows);
  const activeFlowId = useFlowStore((s) => s.activeFlowId);
  const setActiveFlowId = useFlowStore((s) => s.setActiveFlowId);
  const nodeTypes = useFlowStore((s) => s.nodeTypes);
  const runEvents = useFlowStore((s) => s.runEvents);
  const nodeRunStatus = useFlowStore((s) => s.nodeRunStatus);
  const runLogOpen = useFlowStore((s) => s.runLogOpen);
  const setRunLogOpen = useFlowStore((s) => s.setRunLogOpen);
  const addRunEvent = useFlowStore((s) => s.addRunEvent);
  const loadFlows = useFlowStore((s) => s.loadFlows);
  const loadNodeTypes = useFlowStore((s) => s.loadNodeTypes);
  const toggleFlowEnabled = useFlowStore((s) => s.toggleFlowEnabled);

  const selectedAgentId = useAgentStore((s) => s.selectedAgentId);
  const selectedSessionId = useAgentStore((s) => s.selectedSessionId);
  const selectedAgentName = useAgentStore((s) => s.selectedAgentName);
  const visitedAgents = useAgentStore((s) => s.visitedAgents);
  const agentListKey = useAgentStore((s) => s.agentListKey);
  const selectSession = useAgentStore((s) => s.selectSession);
  const removeVisitedAgent = useAgentStore((s) => s.removeVisited);
  const bumpAgentListKey = useAgentStore((s) => s.bumpAgentListKey);

  const activeView = useViewStore((s) => s.activeView);
  const setActiveView = useViewStore((s) => s.setActiveView);
  const selectedNodeId = useViewStore((s) => s.selectedNodeId);
  const setSelectedNodeId = useViewStore((s) => s.setSelectedNodeId);
  const selectedPromptId = useViewStore((s) => s.selectedPromptId);
  const setSelectedPromptId = useViewStore((s) => s.setSelectedPromptId);
  const promptListKey = useViewStore((s) => s.promptListKey);
  const bumpPromptListKey = useViewStore((s) => s.bumpPromptListKey);
  const sidebarCollapsed = useViewStore((s) => s.sidebarCollapsed);
  const setSidebarCollapsed = useViewStore((s) => s.setSidebarCollapsed);
  const showSettings = useViewStore((s) => s.showSettings);
  const setShowSettings = useViewStore((s) => s.setShowSettings);

  // --- Central flow state (extracted hook) ---
  const activeFlowIdRef = useRef(activeFlowId);
  activeFlowIdRef.current = activeFlowId;

  const dispatchApi = useMemo(() => ({
    onSaveComplete: loadFlows,
    updateFlow: api.updateFlow,
    getFlow: api.getFlow,
  }), [loadFlows]);

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
  const handleRunEvent = useCallback((event: import("./types/flow").RunEvent) => {
    addRunEvent(event);

    if (event.event_type === "run_completed" || event.event_type === "run_failed") {
      if (clearTimer.current) clearTimeout(clearTimer.current);
      clearTimer.current = setTimeout(() => useFlowStore.getState().setNodeRunStatus({}), 10000);
    }
  }, [addRunEvent]);

  useEffect(() => {
    if (!activeFlowId) return;
    useFlowStore.getState().clearRunEvents();
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
  }, [loadFlows, loadNodeTypes]);

  // --- File change subscription (SSE) ---
  useEffect(() => {
    const cleanup = api.subscribeToChanges((event) => {
      if (event.resource_type === "flow") {
        loadFlows();
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
        bumpAgentListKey();
      } else if (event.resource_type === "prompt") {
        bumpPromptListKey();
      }
    });
    return cleanup;
  }, [dispatchFlowUpdate, loadFlows, bumpAgentListKey, bumpPromptListKey]);

  const handleReconnect = useCallback(() => {
    loadFlows();
    loadNodeTypes();
  }, [loadFlows, loadNodeTypes]);

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
    await selectSession(agentId, sessionId);
    setSelectedNodeId(null);
    setActiveView("agent-workspace");
  }, [selectSession, setSelectedNodeId, setActiveView]);

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

  const handleCanvasChange = useCallback((updates: { nodes: FlowNode[]; edges: FlowEdge[] }) => {
    dispatchFlowUpdate("canvas", updates);
  }, [dispatchFlowUpdate]);

  const handleEditorChange = useCallback((text: string) => {
    try {
      const parsed = JSON.parse(text) as Flow;
      if (!Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges)) return;
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
  }, [setSelectedNodeId]);

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
    await toggleFlowEnabled(flowId);
    if (activeFlowMeta && activeFlowMeta.id === flowId) {
      const flow = useFlowStore.getState().flows.find((f) => f.id === flowId);
      if (flow) {
        dispatchFlowUpdate("app", { enabled: flow.enabled });
      }
    }
  };

  const handleSaveSettings = () => {
    api.setServerUrl(serverUrl);
    setShowSettings(false);
    loadFlows();
    loadNodeTypes();
  };

  return (
    <AuthGate>
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
            onRunEventsClear={() => useFlowStore.getState().clearRunEvents()}
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
              bumpPromptListKey();
              setActiveView("flow-editor");
            }}
            onBack={handleBackToFlow}
            onTitleChanged={() => {
              bumpPromptListKey();
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
                removeVisitedAgent(agentId);
                bumpAgentListKey();
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
    </AuthGate>
  );
}
