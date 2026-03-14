import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "./api/client";
import { checkSetupStatus } from "./api/client";
import { log } from "./api/logger";
import SetupScreen from "./components/SetupScreen";
import type { Flow, FlowNode, FlowEdge } from "./types/flow";
import TopBar from "./components/TopBar";
import Sidebar from "./components/Sidebar";
import FlowWorkspaceView from "./components/FlowWorkspaceView";
import AgentDetailView from "./components/AgentDetailView";
import PromptEditorView from "./components/PromptEditorView";
import WorkflowsView from "./components/WorkflowsView";
import { AgentListPage } from "./components/AgentListPage";
import { AgentDetailPage } from "./components/AgentDetailPage";
import OrgChart from "./components/OrgChart";
import CreateWorkspaceDialog from "./components/CreateWorkspaceDialog";
import { useGlobalPermissions } from "./hooks/useGlobalPermissions";
import { type CanvasHandle } from "./components/Canvas";
import type { NodeTypeSchema } from "./types/flow";
import { OrgProvider } from "./contexts/OrgContext";
import { OrgRail } from "./components/OrgRail";
import { NewAgentDialog } from "./components/NewAgentDialog";

// ---------------------------------------------------------------------------
// Workflow YAML → Flow conversion (reusable)
// ---------------------------------------------------------------------------

/** Auto-wire edges: connect nodes sequentially in pipeline order. */
function autoWireEdgesFromNodes(nodes: FlowNode[]): FlowEdge[] {
  const ORDER: Record<string, number> = { trigger: 0, source: 1, executor: 2, sink: 3 };
  const sorted = [...nodes].sort(
    (a, b) => (ORDER[a.node_type] ?? 9) - (ORDER[b.node_type] ?? 9)
  );
  return sorted.slice(0, -1).map((n, i) => ({
    id: `edge-${i}`,
    source: n.id,
    target: sorted[i + 1].id,
  }));
}

/**
 * Convert raw workflow data (from `getWorkflow`) into a Flow object.
 * Handles both "flow format" (nodes array) and "template format"
 * (trigger/sources/executors/sinks objects).
 */
export function convertWorkflowDataToFlow(
  data: Record<string, unknown>,
  workspace: string,
  name: string,
): Flow {
  let nodes: FlowNode[];
  let edges: FlowEdge[];

  if (Array.isArray(data.nodes)) {
    // --- Flow format: { nodes: [...], edges: [...] } ---
    const rawNodes = data.nodes as Record<string, unknown>[];
    nodes = rawNodes.map((n, i) => {
      const pos = n.position as { x?: number; y?: number } | undefined;
      return {
        id: (n.id as string) || `node-${i}`,
        node_type: (n.node_type as FlowNode["node_type"]) || "executor",
        kind: (n.kind as string) || "unknown",
        config: (n.config as Record<string, unknown>) || {},
        position: pos && typeof pos.x === "number" && typeof pos.y === "number"
          ? { x: pos.x, y: pos.y }
          : { x: 300 * i, y: 100 },
        label: (n.label as string) || (n.kind as string) || `Node ${i + 1}`,
      };
    });

    if (Array.isArray(data.edges)) {
      edges = (data.edges as Record<string, unknown>[]).map((e, i) => ({
        id: (e.id as string) || `edge-${i}`,
        source: e.source as string,
        target: e.target as string,
      }));
    } else {
      edges = autoWireEdgesFromNodes(nodes);
    }
  } else {
    // --- Template format: { trigger, sources, executors, sinks } ---
    nodes = [];
    let idx = 0;

    if (data.trigger && typeof data.trigger === "object") {
      const t = data.trigger as Record<string, unknown>;
      nodes.push({
        id: `node-${idx}`,
        node_type: "trigger",
        kind: (t.kind as string) || "manual",
        config: (t.config as Record<string, unknown>) || {},
        position: { x: 0, y: 0 },
        label: (t.label as string) || `Trigger: ${(t.kind as string) || "manual"}`,
      });
      idx++;
    }

    const sources = Array.isArray(data.sources) ? data.sources as Record<string, unknown>[] : [];
    for (const s of sources) {
      nodes.push({
        id: `node-${idx}`,
        node_type: "source",
        kind: (s.kind as string) || "unknown",
        config: (s.config as Record<string, unknown>) || {},
        position: { x: 0, y: 0 },
        label: (s.label as string) || `Source: ${(s.kind as string) || "unknown"}`,
      });
      idx++;
    }

    const filters = Array.isArray(data.filters) ? data.filters as Record<string, unknown>[] : [];
    for (const f of filters) {
      nodes.push({
        id: `node-${idx}`,
        node_type: "source",
        kind: (f.kind as string) || "keyword",
        config: (f.config as Record<string, unknown>) || {},
        position: { x: 0, y: 0 },
        label: (f.label as string) || `Filter: ${(f.kind as string) || "keyword"}`,
      });
      idx++;
    }

    const executors = Array.isArray(data.executors) ? data.executors as Record<string, unknown>[] : [];
    for (const e of executors) {
      nodes.push({
        id: `node-${idx}`,
        node_type: "executor",
        kind: (e.kind as string) || "claude-code",
        config: (e.config as Record<string, unknown>) || {},
        position: { x: 0, y: 0 },
        label: (e.label as string) || `Executor: ${(e.kind as string) || "claude-code"}`,
      });
      idx++;
    }

    const sinks = Array.isArray(data.sinks) ? data.sinks as Record<string, unknown>[] : [];
    for (const s of sinks) {
      nodes.push({
        id: `node-${idx}`,
        node_type: "sink",
        kind: (s.kind as string) || "unknown",
        config: (s.config as Record<string, unknown>) || {},
        position: { x: 0, y: 0 },
        label: (s.label as string) || `Sink: ${(s.kind as string) || "unknown"}`,
      });
      idx++;
    }

    nodes.forEach((n, i) => { n.position = { x: 300 * i, y: 100 }; });
    edges = autoWireEdgesFromNodes(nodes);
  }

  return {
    id: `wf::${workspace}::${name}`,
    name: (data.name as string) || name,
    description: (data.description as string) || "",
    enabled: false,
    nodes,
    edges,
    version: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

import { UIProvider, useUI } from "./contexts/UIContext";
import { NavigationProvider, useNavigation } from "./contexts/NavigationContext";
import { FlowProvider, useFlowContext } from "./contexts/FlowContext";
import { RunProvider, useRunContext } from "./contexts/RunContext";
import { WorkflowProvider, useWorkflowContext } from "./contexts/WorkflowContext";
import ConfirmDialog, { useConfirm } from "./components/ConfirmDialog";

/**
 * Inner component that consumes all contexts.
 * Keeps the complex handler functions that orchestrate across contexts.
 */
function AppInner() {
  const globalPermissions = useGlobalPermissions();

  // --- Context hooks ---
  const {
    sidebarCollapsed,
    setSidebarCollapsed,
    showSettings,
    setShowSettings,
    serverUrl,
    setServerUrl,
  } = useUI();

  const {
    activeView,
    setActiveView,
    selectedNodeId,
    setSelectedNodeId,
    selectedAgentId,
    setSelectedAgentId,
    selectedSessionId,
    setSelectedSessionId,
    selectedAgentName,
    setSelectedAgentName,
    visitedAgents,
    setVisitedAgents,
    selectedPromptId,
    setSelectedPromptId,
    editingWorkflow,
    setEditingWorkflow,
  } = useNavigation();

  const {
    flows,
    setFlows,
    activeFlowId,
    setActiveFlowId,
    nodeTypes,
    canonicalFlow,
    updateSignal,
    dispatchFlowUpdate,
    initFlow,
    activeFlowMeta,
    loadFlows,
    loadNodeTypes,
  } = useFlowContext();

  const {
    runEvents,
    setRunEvents,
    nodeRunStatus,
    setNodeRunStatus: _setNodeRunStatus,
    runLogOpen,
    setRunLogOpen,
  } = useRunContext();

  const {
    wfWorkspaces,
    setWfWorkspaces,
    wfActiveWorkspace,
    setWfActiveWorkspace,
    wfWorkflows,
    setWfWorkflows,
    showNewWorkspace,
    setShowNewWorkspace,
    newWorkflowTrigger,
    setNewWorkflowTrigger,
    toggleWorkflowEnabled,
    isWorkflowEnabled,
  } = useWorkflowContext();

  // --- Local state that stays in App ---
  const [agentListKey, setAgentListKey] = useState(0);
  const [showNewAgentDialog, setShowNewAgentDialog] = useState(false);
  const [promptListKey, setPromptListKey] = useState(0);
  const [confirmState, requestConfirm] = useConfirm();

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

  // --- File change subscription (SSE) ---
  const activeFlowIdRef = useRef(activeFlowId);
  activeFlowIdRef.current = activeFlowId;
  const editingWorkflowRef = useRef(editingWorkflow);
  editingWorkflowRef.current = editingWorkflow;

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
      } else if (event.resource_type === "workflow") {
        // Workflow YAML changed externally (e.g. by Claude Code in the terminal)
        // resource_id format: "workspace::name"
        const ew = editingWorkflowRef.current;
        if (ew && event.change_type === "updated") {
          const expectedId = `${ew.workspace}::${ew.name}`;
          if (event.resource_id === expectedId) {
            log("info", `Workflow YAML changed externally: ${event.resource_id}, re-fetching`);
            api.getWorkflow(ew.workspace, ew.name).then((data) => {
              const flow = convertWorkflowDataToFlow(data, ew.workspace, ew.name);
              dispatchFlowUpdate("server", {
                nodes: flow.nodes,
                edges: flow.edges,
                name: flow.name,
                description: flow.description,
              });
            }).catch(() => { /* logged */ });
          }
        }
      }
    });
    return cleanup;
  }, [dispatchFlowUpdate, loadFlows]);

  // --- Load workspaces eagerly on mount so TopBar dropdown is populated ---
  // Call listWorkspaces directly (fast, local-only). Skip setupWorkflows (slow GitHub API).
  // WorkflowsView handles setup when the user navigates there.
  useEffect(() => {
    api.listWorkspaces()
      .then((res) => {
        if (res.workspaces.length > 0) {
          setWfWorkspaces(res.workspaces);
          if (!wfActiveWorkspace) {
            setWfActiveWorkspace(res.workspaces[0]);
          }
        }
      })
      .catch(() => {});
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

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
      setEditingWorkflow(null); // Regular flow, not a workflow
      setActiveView("flow-editor");
    } catch { /* logged */ }
  };

  const openWorkflow = useCallback(async (workspace: string, name: string) => {
    try {
      const data = await api.getWorkflow(workspace, name);
      const flow = convertWorkflowDataToFlow(data, workspace, name);
      setActiveFlowId(flow.id);
      initFlow(flow);
      setSelectedNodeId(null);
      setEditingWorkflow({ workspace, name });
    } catch (e) {
      log("error", `Failed to open workflow ${workspace}/${name}: ${(e as Error).message}`);
    }
  }, [initFlow, setActiveFlowId, setSelectedNodeId, setEditingWorkflow]);

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
  }, [setSelectedAgentId, setSelectedSessionId, setSelectedAgentName, setVisitedAgents, setSelectedNodeId, setActiveView]);

  /** Navigate to the new Paperclip-style agent detail page */
  const handleSelectAgent = useCallback(async (agentId: string) => {
    try {
      const agent = await api.getAgent(agentId);
      // Get or create a session for the terminal
      let sessionId: string;
      try {
        const sessions = await api.listAgentSessions(agentId);
        if (sessions.sessions.length > 0) {
          sessionId = sessions.sessions[0].session_id;
        } else {
          const result = await api.newAgentSession(agentId);
          sessionId = result.session_id;
        }
      } catch {
        const result = await api.newAgentSession(agentId);
        sessionId = result.session_id;
      }
      setSelectedAgentId(agentId);
      setSelectedSessionId(sessionId);
      setSelectedAgentName(agent.name);
      setActiveView("agent-detail");
    } catch { /* logged */ }
  }, [setSelectedAgentId, setSelectedSessionId, setSelectedAgentName, setActiveView]);

  const handleBackToFlow = () => {
    if (activeView === "agent-detail" || activeView === "agent-workspace" || activeView === "org-chart") {
      setActiveView("agent-list");
    } else {
      setActiveView("flow-editor");
    }
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
    // In desktop mode, server URL is not configurable — we use Tauri IPC
    setShowSettings(false);
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
        onNavigate={(view) => {
          setEditingWorkflow(null);
          setActiveView(view);
        }}
        editingWorkflow={editingWorkflow}
        workspaces={wfWorkspaces}
        activeWorkspace={wfActiveWorkspace}
        onSelectWorkspace={(ws) => setWfActiveWorkspace(ws)}
        onCreateWorkspace={() => setShowNewWorkspace(true)}
        onRunWorkflow={editingWorkflow ? async () => {
          try {
            setRunLogOpen(true);
            await api.runWorkflow(editingWorkflow.workspace, editingWorkflow.name);
            log("info", `Triggered workflow ${editingWorkflow.workspace}/${editingWorkflow.name}`);
          } catch (e) {
            log("error", `Run failed: ${typeof e === "string" ? e : (e as Error).message}`);
          }
        } : undefined}
        onSaveWorkflow={editingWorkflow ? async () => {
          if (!canonicalFlow) return;
          try {
            await api.saveWorkflow(editingWorkflow.workspace, editingWorkflow.name, {
              name: canonicalFlow.name,
              description: canonicalFlow.description || "",
              nodes: canonicalFlow.nodes,
              edges: canonicalFlow.edges,
            });
            log("info", `Saved workflow ${editingWorkflow.workspace}/${editingWorkflow.name}`);
          } catch (e) {
            log("error", `Save failed: ${(e as Error).message}`);
          }
        } : undefined}
        onPublish={editingWorkflow ? async () => {
          if (!canonicalFlow) return;
          try {
            await api.publishWorkflow(editingWorkflow.workspace, editingWorkflow.name, {
              name: canonicalFlow.name,
              description: canonicalFlow.description || "",
              nodes: canonicalFlow.nodes,
              edges: canonicalFlow.edges,
            });
            log("info", `Published workflow ${editingWorkflow.workspace}/${editingWorkflow.name}`);
          } catch (e) {
            log("error", `Publish failed: ${(e as Error).message}`);
          }
        } : undefined}
      />
      <div className="app-layout">
        {(activeView === "agent-list" || activeView === "agent-detail" || activeView === "agent-workspace" || activeView === "org-chart") && (
          <OrgRail />
        )}
        {sidebarCollapsed ? (
          <div className="sidebar-collapsed" onClick={() => setSidebarCollapsed(false)}>
            <span className="sidebar-collapsed-icon">◧</span>
            <span className="sidebar-collapsed-label">Nav</span>
          </div>
        ) : (
          <Sidebar
            selectedAgentId={selectedAgentId}
            onSelectAgent={handleSelectAgent}
            agentListKey={agentListKey}
            onAgentCreated={(id) => {
              handleSelectAgent(id);
            }}
            selectedPromptId={selectedPromptId}
            onSelectPrompt={handleSelectPrompt}
            promptListKey={promptListKey}
            activeView={activeView}
            onCollapse={() => setSidebarCollapsed(true)}
            activeFlowId={activeFlowId}
            nodeTypes={nodeTypes}
            onGrab={handleGrab}
            activeWorkspace={wfActiveWorkspace}
            workflows={wfWorkflows}
            onSelectWorkflow={openWorkflow}
            onCreateWorkflow={() => setNewWorkflowTrigger((n) => n + 1)}
            onToggleWorkflowEnabled={toggleWorkflowEnabled}
            onRunWorkflow={async (workspace, name) => {
              try {
                setRunLogOpen(true);
                await api.runWorkflow(workspace, name);
                log("info", `Triggered workflow ${workspace}/${name}`);
              } catch (e) {
                log("error", `Run failed: ${typeof e === "string" ? e : (e as Error).message}`);
              }
            }}
            isWorkflowEnabled={isWorkflowEnabled}
            onDeleteWorkflow={async (workspace, name) => {
              const ok = await requestConfirm(`Delete "${name}"?`, "This workflow will be permanently removed.");
              if (!ok) return;
              try {
                await api.deleteWorkflow(workspace, name);
                setWfWorkflows((prev) => prev.filter((wf) => !(wf.workspace === workspace && wf.name === name)));
                if (editingWorkflow?.workspace === workspace && editingWorkflow?.name === name) {
                  setEditingWorkflow(null);
                  setActiveFlowId(null);
                }
              } catch (e) {
                console.error("Failed to delete workflow:", e);
              }
            }}
            editingWorkflow={editingWorkflow}
          />
        )}

        <div style={{ display: activeView === "flow-editor" || editingWorkflow ? "contents" : "none" }}>
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
            editingWorkflow={editingWorkflow}
            openWorkflow={openWorkflow}
            activeWorkspace={wfActiveWorkspace}
            onWorkflowCreated={(ws, name) => {
              // Refresh the workflows list so sidebar picks up the new entry
              api.listWorkflows(ws).then((res) => {
                setWfWorkflows(res.workflows);
              }).catch(() => {});
            }}
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

        {/* Agent List Page (Paperclip style) */}
        {activeView === "agent-list" && (
          <AgentListPage
            onSelectAgent={handleSelectAgent}
            onCreateAgent={() => setShowNewAgentDialog(true)}
            refreshKey={agentListKey}
          />
        )}
        {showNewAgentDialog && (
          <NewAgentDialog
            onClose={() => setShowNewAgentDialog(false)}
            onCreated={(id) => {
              setShowNewAgentDialog(false);
              setAgentListKey((k) => k + 1);
              handleSelectAgent(id);
            }}
          />
        )}

        {/* Org Chart */}
        {activeView === "org-chart" && <OrgChart onSelectAgent={handleSelectAgent} />}

        {/* Agent Detail Page with 3 tabs (Paperclip style) */}
        {activeView === "agent-detail" && selectedAgentId && selectedSessionId && (
          <AgentDetailPage
            agentId={selectedAgentId}
            sessionId={selectedSessionId}
            onBack={() => setActiveView("agent-list")}
            onDeleted={() => {
              setSelectedAgentId(null);
              setSelectedSessionId(null);
              setSelectedAgentName(null);
              setAgentListKey((k) => k + 1);
              setActiveView("agent-list");
            }}
          />
        )}

        {/* Legacy agent workspace view (kept for backward compatibility) */}
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
                setActiveView("agent-list");
              }}
            />
          </div>
        ))}

        <div style={{ display: activeView === "workflows" && !editingWorkflow ? "contents" : "none" }}>
          <WorkflowsView
            onOpenWorkflow={openWorkflow}
            activeWorkspace={wfActiveWorkspace}
            onSelectWorkspace={(ws) => setWfActiveWorkspace(ws)}
            onWorkflowsChanged={(_workspaces, workflows) => {
              // Workspaces are managed by AppInner's eager load + CreateWorkspaceDialog.
              // Only sync workflows here — not workspaces — to avoid race conditions.
              setWfWorkflows(workflows);
            }}
            newWorkflowTrigger={newWorkflowTrigger}
          />
        </div>
      </div>

      <Dialog open={showSettings} onOpenChange={setShowSettings}>
        <DialogContent className="bg-[var(--bg-secondary)] border-[var(--border)] text-[var(--text)]">
          <DialogHeader>
            <DialogTitle>Settings</DialogTitle>
          </DialogHeader>
          <div className="form-group">
            <p className="text-sm text-[var(--text-secondary)]">
              Connected via Tauri IPC (desktop mode). No server URL configuration needed.
            </p>
          </div>
          <DialogFooter>
            <Button onClick={handleSaveSettings}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <CreateWorkspaceDialog
        open={showNewWorkspace}
        onOpenChange={setShowNewWorkspace}
        onCreated={async (name) => {
          // Optimistically add workspace and select it
          setWfWorkspaces((prev) => [...prev, name].sort());
          setWfActiveWorkspace(name);
          // Switch to workflows view so user sees it
          setEditingWorkflow(null);
          setActiveView("workflows");
          // Refresh full workspace list from backend
          try {
            const res = await api.listWorkspaces();
            setWfWorkspaces(res.workspaces);
          } catch {
            // optimistic update already applied
          }
        }}
      />

      <ConfirmDialog {...confirmState} />
    </div>
  );
}

/**
 * App root: wraps the inner component with context providers.
 * RunProvider needs activeFlowId, so we use a bridge component.
 */
function AppWithRunProvider() {
  const { activeFlowId } = useFlowContext();
  return (
    <RunProvider activeFlowId={activeFlowId}>
      <WorkflowProvider>
        <AppInner />
      </WorkflowProvider>
    </RunProvider>
  );
}

export default function App() {
  const [setupComplete, setSetupComplete] = useState<boolean | null>(null);

  useEffect(() => {
    checkSetupStatus()
      .then((status) => setSetupComplete(status.setup_complete))
      .catch(() => setSetupComplete(false));
  }, []);

  if (setupComplete === null) {
    return (
      <div className="welcome-screen">
        <div className="welcome-content">
          <h1 className="welcome-title">Cthulu Studio</h1>
          <div className="welcome-dots">
            <span className="welcome-dot" />
            <span className="welcome-dot" />
            <span className="welcome-dot" />
          </div>
        </div>
      </div>
    );
  }

  if (!setupComplete) {
    return <SetupScreen onComplete={() => setSetupComplete(true)} />;
  }

  return (
    <UIProvider>
      <OrgProvider>
        <NavigationProvider>
          <FlowProvider>
            <AppWithRunProvider />
          </FlowProvider>
        </NavigationProvider>
      </OrgProvider>
    </UIProvider>
  );
}
