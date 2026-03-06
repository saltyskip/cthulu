import { useState, useCallback, useRef, useEffect, useMemo, type RefObject } from "react";
import { STUDIO_ASSISTANT_ID, type Flow, type FlowNode, type FlowEdge, type RunEvent } from "../types/flow";
import { listAgentSessions, newAgentSession, updateAgent } from "../api/client";
import type { UpdateSignal } from "../hooks/useFlowDispatch";
import { X, EyeOff, Plus } from "lucide-react";
import Canvas, { type CanvasHandle } from "./Canvas";
import FlowEditor, { type FlowEditorHandle } from "./FlowEditor";
import RunLog from "./RunLog";
import AgentChatView, { useAgentChat } from "./AgentChatView";
import NodeConfigPanel from "./NodeConfigPanel";
import ErrorBoundary from "./ErrorBoundary";

/**
 * Condensed workflow-builder skill injected into the studio-assistant's system prompt.
 * Teaches the agent to output structured JSON code blocks the Studio can parse.
 */
const WORKFLOW_SKILL_PROMPT = `You can help users build workflow pipelines. When asked to create a workflow, follow this protocol:

1. Ask for a workflow NAME if not provided.
2. Clarify sources, schedule, and destinations.
3. Show a text PREVIEW of the pipeline.
4. Ask "Shall I create this workflow?" — NEVER create without confirmation.
5. On confirmation, output a JSON code block with the flow definition.

OUTPUT FORMAT — use a fenced json code block:
\`\`\`json
{
  "action": "create_flow",
  "name": "kebab-case-name",
  "description": "What this workflow does",
  "nodes": [
    { "node_type": "trigger", "kind": "cron", "label": "Every 4h", "config": { "schedule": "0 */4 * * *" } },
    { "node_type": "source", "kind": "rss", "label": "RSS: Example", "config": { "url": "https://example.com/feed", "limit": 20 } },
    { "node_type": "executor", "kind": "claude-code", "label": "Summarizer", "config": { "prompt": "Summarize:\\n\\n{{content}}" } },
    { "node_type": "sink", "kind": "slack", "label": "Post to Slack", "config": { "channel": "#general" } }
  ],
  "edges": "auto"
}
\`\`\`

NODE TYPES:
- trigger: cron (schedule), github-pr (repo), webhook, manual
- source: rss (url, limit?, keywords?), web-scrape (url), web-scraper (url, items_selector, title_selector, url_selector), github-merged-prs (repos, since_days?), market-data (no config), google-sheets (spreadsheet_id, range, service_account_key_env)
- filter: keyword (keywords, mode?, field?)
- executor: claude-code (prompt REQUIRED, permissions?, working_dir?)
- sink: slack (webhook_url_env?, bot_token_env?, channel?), notion (token_env, database_id)

EDGE WIRING: trigger→source, source→executor (or source→filter→executor), executor→sink. "edges": "auto" handles this.

PROMPT VARIABLES: {{content}}, {{item_count}}, {{timestamp}}, {{market_data}}, {{diff}}, {{pr_number}}, {{pr_title}}, {{repo}}`;

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

const ALL_TABS: { id: BottomTab; label: string }[] = [
  { id: "log", label: "Run Log" },
  { id: "terminal", label: "Terminal" },
];

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
  const studioChat = useAgentChat(STUDIO_ASSISTANT_ID, studioSessionId ?? "");

  // Tab visibility: which tabs are shown (VS Code-style toggle)
  const [visibleTabs, setVisibleTabs] = useState<Set<BottomTab>>(
    () => new Set<BottomTab>(["log", "terminal"])
  );
  // Context menu state for right-click on tabs
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; tab: BottomTab } | null>(null);
  // "Re-show" menu for adding hidden tabs back
  const [showAddMenu, setShowAddMenu] = useState(false);
  const addMenuRef = useRef<HTMLDivElement>(null);

  const hiddenTabs = useMemo(
    () => ALL_TABS.filter((t) => !visibleTabs.has(t.id)),
    [visibleTabs]
  );

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

  // Ensure the studio-assistant agent has the workflow-builder skill prompt
  const skillInjectedRef = useRef(false);
  useEffect(() => {
    if (skillInjectedRef.current) return;
    skillInjectedRef.current = true;
    updateAgent(STUDIO_ASSISTANT_ID, {
      append_system_prompt: WORKFLOW_SKILL_PROMPT,
    }).catch(() => { /* agent may not exist yet */ });
  }, []);

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

  // Open bottom pane when run log is requested — also re-show if hidden
  useEffect(() => {
    if (runLogOpen && !bottomOpen) {
      setVisibleTabs((prev) => new Set(prev).add("log"));
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

  // Right-click on a tab → show context menu
  const handleTabContextMenu = useCallback(
    (e: React.MouseEvent, tab: BottomTab) => {
      e.preventDefault();
      setCtxMenu({ x: e.clientX, y: e.clientY, tab });
    },
    []
  );

  // Hide a tab (from context menu)
  const handleHideTab = useCallback(
    (tab: BottomTab) => {
      setCtxMenu(null);
      setVisibleTabs((prev) => {
        const next = new Set(prev);
        next.delete(tab);
        // If no tabs remain visible, close the panel
        if (next.size === 0) {
          setBottomOpen(false);
          onRunLogClose();
          return next;
        }
        // If the hidden tab was active, switch to the first remaining visible tab
        if (bottomTab === tab) {
          const firstVisible = ALL_TABS.find((t) => next.has(t.id));
          if (firstVisible) setBottomTab(firstVisible.id);
        }
        return next;
      });
    },
    [bottomTab, onRunLogClose]
  );

  // Re-show a hidden tab
  const handleShowTab = useCallback(
    (tab: BottomTab) => {
      setShowAddMenu(false);
      setVisibleTabs((prev) => new Set(prev).add(tab));
      setBottomTab(tab);
      setBottomOpen(true);
    },
    []
  );

  // Close context menu on outside click
  useEffect(() => {
    if (!ctxMenu) return;
    const handler = () => setCtxMenu(null);
    window.addEventListener("click", handler);
    return () => window.removeEventListener("click", handler);
  }, [ctxMenu]);

  // Close add menu on outside click
  useEffect(() => {
    if (!showAddMenu) return;
    const handler = (e: MouseEvent) => {
      if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node)) {
        setShowAddMenu(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [showAddMenu]);

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
              {ALL_TABS.filter((t) => visibleTabs.has(t.id)).map((t) => (
                <button
                  key={t.id}
                  className={`flow-workspace-tab${bottomTab === t.id ? " active" : ""}`}
                  onClick={() => setBottomTab(t.id)}
                  onContextMenu={(e) => handleTabContextMenu(e, t.id)}
                >
                  {t.label}
                </button>
              ))}
              {hiddenTabs.length > 0 && (
                <div className="flow-workspace-tab-add-wrap" ref={addMenuRef}>
                  <button
                    className="flow-workspace-tab-add"
                    onClick={() => setShowAddMenu((v) => !v)}
                    title="Show hidden tabs"
                  >
                    <Plus size={14} />
                  </button>
                  {showAddMenu && (
                    <div className="flow-workspace-tab-dropdown">
                      {hiddenTabs.map((t) => (
                        <button
                          key={t.id}
                          className="flow-workspace-tab-dropdown-item"
                          onClick={() => handleShowTab(t.id)}
                        >
                          {t.label}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              )}
              <div className="spacer" />
              <button className="flow-workspace-tab-close" onClick={handleBottomClose}>
                <X size={14} />
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
                <StudioAssistantChat
                  key={`workspace-chat:${STUDIO_ASSISTANT_ID}::${studioSessionId}`}
                  sessionId={studioSessionId}
                />
              )}
            </div>
          </div>
        </>
      )}

      {/* Toggle bar when bottom is closed */}
      {!bottomOpen && flowId && (
        <div className="flow-workspace-bottom-toggle">
          {ALL_TABS.filter((t) => visibleTabs.has(t.id)).map((t) => (
            <button
              key={t.id}
              className="flow-workspace-tab"
              onClick={() => { setBottomOpen(true); setBottomTab(t.id); }}
              onContextMenu={(e) => handleTabContextMenu(e, t.id)}
            >
              {t.label}
            </button>
          ))}
          {hiddenTabs.length > 0 && (
            <div className="flow-workspace-tab-add-wrap" ref={addMenuRef}>
              <button
                className="flow-workspace-tab-add"
                onClick={() => setShowAddMenu((v) => !v)}
                title="Show hidden tabs"
              >
                <Plus size={14} />
              </button>
              {showAddMenu && (
                <div className="flow-workspace-tab-dropdown">
                  {hiddenTabs.map((t) => (
                    <button
                      key={t.id}
                      className="flow-workspace-tab-dropdown-item"
                      onClick={() => handleShowTab(t.id)}
                    >
                      {t.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Right-click context menu */}
      {ctxMenu && (
        <div
          className="flow-workspace-tab-ctx"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="flow-workspace-tab-ctx-item"
            onClick={() => handleHideTab(ctxMenu.tab)}
          >
            <EyeOff size={12} />
            Hide Tab
          </button>
        </div>
      )}
    </div>
  );
}

/** Wrapper so useAgentChat can be called unconditionally */
function StudioAssistantChat({ sessionId }: { sessionId: string }) {
  const chat = useAgentChat(STUDIO_ASSISTANT_ID, sessionId);
  const emptyPerms: never[] = [];
  const noop = () => {};
  return <AgentChatView chat={chat} pendingPermissions={emptyPerms} onPermissionResponse={noop} />;
}
