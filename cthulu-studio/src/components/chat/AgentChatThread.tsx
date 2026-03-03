import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  ThreadPrimitive,
  ComposerPrimitive,
  type ThreadMessageLike,
} from "@assistant-ui/react";
import {
  CompactAssistantMessage,
  CompactUserMessage,
} from "../ChatPrimitives";
import { AskUserQuestionToolUI } from "../ToolRenderers";
import { FilePreviewContext } from "./FilePreviewContext";
import { extractFileOps, extractPlans, extractLatestTodos } from "./chatUtils";
import FilePreviewPanel from "./FilePreviewPanel";
import StickyTodoPanel from "./StickyTodoPanel";
import type { ImageAttachment, DebugEvent } from "./useAgentChat";

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

function DebugEventRow({ ev }: { ev: DebugEvent }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div className={`fr-debug-event ${ev.error ? "fr-debug-event-error" : ""}`}>
      <div className="fr-debug-event-row" onClick={() => setExpanded((v) => !v)}>
        <span className="fr-debug-expand">{expanded ? "▾" : "▸"}</span>
        <span className="fr-debug-ts">{new Date(ev.ts).toLocaleTimeString()}</span>
        <span className={`fr-debug-badge fr-debug-badge-${ev.type}`}>{ev.type}</span>
        {!expanded && (
          <span className="fr-debug-preview">
            {ev.data.length > 80 ? ev.data.slice(0, 80) + "…" : ev.data}
          </span>
        )}
      </div>
      {expanded && (
        <pre className="fr-debug-json">{prettyJson(ev.data)}</pre>
      )}
    </div>
  );
}

interface AgentChatThreadProps {
  messages: ThreadMessageLike[];
  isStreaming: boolean;
  resultMeta: { cost: number; turns: number } | null;
  isDone: boolean;
  onNew: (message: { content: unknown; role?: string }) => Promise<void>;
  onCancel: () => void;
  attachments: ImageAttachment[];
  onAddFiles: (files: FileList | File[]) => void;
  onRemoveAttachment: (id: string) => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
  debugMode: boolean;
  debugEvents: DebugEvent[];
  onToggleDebug: () => void;
  onClearDebug: () => void;
}

export default function AgentChatThread({
  messages,
  isStreaming,
  resultMeta,
  isDone,
  onNew,
  onCancel,
  attachments,
  onAddFiles,
  onRemoveAttachment,
  fileInputRef,
  debugMode,
  debugEvents,
  onToggleDebug,
  onClearDebug,
}: AgentChatThreadProps) {
  const [dragOver, setDragOver] = useState(false);
  // Stable references prevent useExternalStoreRuntime from flushing its
  // internal converter cache on every render — without this, the runtime
  // re-converts ALL messages each frame, which can delay tool-call rendering.
  const convertMessage = useCallback((msg: ThreadMessageLike) => msg, []);
  const handleNew = useCallback(
    async (message: { content: unknown; role?: string }) => { await onNew(message); },
    [onNew],
  );
  const handleCancel = useCallback(async () => { onCancel(); }, [onCancel]);
  const handleAddToolResult = useCallback(
    async (options: { result: unknown }) => {
      // When a tool renderer (e.g. AskUserQuestion) submits a result,
      // send it as a user message to the Claude process.
      const answer = typeof options.result === "object" && options.result !== null
        ? (options.result as Record<string, unknown>).answer ?? JSON.stringify(options.result)
        : String(options.result);
      await onNew({ content: answer as string });
    },
    [onNew],
  );

  const rawTodos = useMemo(() => extractLatestTodos(messages), [messages]);
  // Track whether todos were auto-resolved by a done event.
  // Persists across isDone resets; only clears when a new TodoWrite arrives.
  const todosResolvedRef = useRef(false);
  const prevRawTodosRef = useRef(rawTodos);
  if (rawTodos !== prevRawTodosRef.current) {
    // New TodoWrite arrived — reset resolved flag
    todosResolvedRef.current = false;
    prevRawTodosRef.current = rawTodos;
  }
  if (isDone && !todosResolvedRef.current) {
    todosResolvedRef.current = true;
  }
  const latestTodos = useMemo(() => {
    if (!rawTodos || !todosResolvedRef.current) return rawTodos;
    return rawTodos.map((t) => t.status === "completed" ? t : { ...t, status: "completed" });
  }, [rawTodos, todosResolvedRef.current]);
  const fileOps = useMemo(() => extractFileOps(messages), [messages]);
  const plans = useMemo(() => extractPlans(messages), [messages]);
  const [selectedFileId, setSelectedFileId] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [previewWidth, setPreviewWidth] = useState(480);
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);

  // Auto-select latest artifact (file op or plan) as new ones arrive
  const prevOpsLenRef = useRef(0);
  const prevPlansLenRef = useRef(0);
  useEffect(() => {
    const totalPrev = prevOpsLenRef.current + prevPlansLenRef.current;
    const totalNow = fileOps.length + plans.length;
    if (totalNow > totalPrev) {
      // Select the most recently added artifact
      if (plans.length > prevPlansLenRef.current) {
        setSelectedFileId(plans[plans.length - 1].toolCallId);
      } else if (fileOps.length > prevOpsLenRef.current) {
        setSelectedFileId(fileOps[fileOps.length - 1].toolCallId);
      }
      if (!previewOpen) setPreviewOpen(true);
    }
    prevOpsLenRef.current = fileOps.length;
    prevPlansLenRef.current = plans.length;
  }, [fileOps, plans, previewOpen]);

  const handleDividerDrag = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current = { startX: e.clientX, startW: previewWidth };

    const onMove = (ev: MouseEvent) => {
      if (!dragRef.current) return;
      const delta = dragRef.current.startX - ev.clientX;
      const maxW = Math.max(240, window.innerWidth - 300);
      const newW = Math.min(maxW, Math.max(240, dragRef.current.startW + delta));
      setPreviewWidth(newW);
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
  }, [previewWidth]);

  const runtime = useExternalStoreRuntime({
    isRunning: isStreaming,
    messages,
    convertMessage,
    onNew: handleNew,
    onCancel: handleCancel,
    onAddToolResult: handleAddToolResult,
  });

  const hasArtifacts = fileOps.length > 0 || plans.length > 0;
  const showRightPanel = hasArtifacts || debugMode;
  const [rightTab, setRightTab] = useState<"artifacts" | "debug">("artifacts");

  // Auto-switch to debug tab when toggled on
  const prevDebugRef = useRef(debugMode);
  useEffect(() => {
    if (debugMode && !prevDebugRef.current) setRightTab("debug");
    prevDebugRef.current = debugMode;
  }, [debugMode]);

  const handleFileSelect = useCallback((toolCallId: string) => {
    setSelectedFileId(toolCallId);
    if (!previewOpen) setPreviewOpen(true);
    setRightTab("artifacts");
  }, [previewOpen]);

  // Keyboard shortcut: Cmd/Ctrl+Shift+D to toggle debug mode
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "D") {
        e.preventDefault();
        onToggleDebug();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onToggleDebug]);

  const debugScrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (debugMode && debugScrollRef.current) {
      debugScrollRef.current.scrollTop = debugScrollRef.current.scrollHeight;
    }
  }, [debugMode, debugEvents]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes("Files")) setDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
    if (e.dataTransfer.files.length > 0) onAddFiles(e.dataTransfer.files);
  }, [onAddFiles]);

  const handlePaste = useCallback((e: React.ClipboardEvent) => {
    const files = e.clipboardData.files;
    if (files.length > 0) {
      const imageFiles = Array.from(files).filter((f) => f.type.startsWith("image/"));
      if (imageFiles.length > 0) {
        onAddFiles(imageFiles);
      }
    }
  }, [onAddFiles]);

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FilePreviewContext.Provider value={handleFileSelect}>
      <AskUserQuestionToolUI />
      <div
        className={`fr-wrap ${showRightPanel ? "fr-wrap-split" : ""} ${dragOver ? "fr-drag-over" : ""}`}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <div className="fr-wrap-chat">
          <ThreadPrimitive.Root className="fr-thread">
            <ThreadPrimitive.Viewport className="fr-viewport">
              <ThreadPrimitive.Messages
                components={{
                  UserMessage: CompactUserMessage,
                  AssistantMessage: CompactAssistantMessage,
                }}
              />
            </ThreadPrimitive.Viewport>
          </ThreadPrimitive.Root>

          {isStreaming && (
            <div className="fr-busy">
              <span className="fr-busy-dot" />
              <span>Thinking…</span>
            </div>
          )}

          {latestTodos && latestTodos.length > 0 && latestTodos.some((t) => t.status !== "completed") && (
            <StickyTodoPanel todos={latestTodos} />
          )}

          {isDone && !isStreaming && (
            <div className="fr-done-banner">
              <span className="fr-done-check">✓</span>
              <span className="fr-done-text">Done</span>
              {resultMeta && (
                <span className="fr-done-meta">{resultMeta.turns} turn{resultMeta.turns !== 1 ? "s" : ""} · ${resultMeta.cost.toFixed(4)}</span>
              )}
            </div>
          )}

          {attachments.length > 0 && (
            <div className="fr-attachments">
              {attachments.map((a) => (
                <div key={a.id} className="fr-attachment">
                  <img src={a.preview} alt={a.file.name} className="fr-attachment-thumb" />
                  <span className="fr-attachment-name">{a.file.name}</span>
                  <button className="fr-attachment-remove" onClick={() => onRemoveAttachment(a.id)}>×</button>
                </div>
              ))}
            </div>
          )}

          <div className="ac-footer" onPaste={handlePaste}>
            <ComposerPrimitive.Root>
              <button
                className={`ac-btn ac-btn-debug ${debugMode ? "ac-btn-debug-active" : ""}`}
                onClick={onToggleDebug}
                title="Toggle debug mode (Cmd+Shift+D)"
                type="button"
              >
                🐛
              </button>
              <button
                className="ac-btn ac-btn-attach"
                onClick={() => fileInputRef.current?.click()}
                title="Attach image"
                type="button"
              >
                📎
              </button>
              <input
                ref={fileInputRef}
                type="file"
                accept="image/*"
                multiple
                style={{ display: "none" }}
                onChange={(e) => {
                  if (e.target.files) onAddFiles(e.target.files);
                  e.target.value = "";
                }}
              />
              <ComposerPrimitive.Input
                placeholder="Send a message..."
                autoFocus
              />
              {isStreaming ? (
                <button className="ac-btn ac-btn-stop" onClick={onCancel}>
                  Stop
                </button>
              ) : (
                <ComposerPrimitive.Send className="ac-btn">
                  Send
                </ComposerPrimitive.Send>
              )}
            </ComposerPrimitive.Root>
          </div>
        </div>

        {showRightPanel && previewOpen && (
          <>
            <div className="fr-preview-divider" onMouseDown={handleDividerDrag} />
            <div className="fr-preview" style={{ width: previewWidth, flex: `0 0 ${previewWidth}px` }}>
              <div className="fr-preview-topbar">
                {hasArtifacts && (
                  <button
                    className={`fr-tab ${rightTab === "artifacts" ? "fr-tab-active" : ""}`}
                    onClick={() => setRightTab("artifacts")}
                  >
                    Artifacts <span className="fr-tab-count">{fileOps.length + plans.length}</span>
                  </button>
                )}
                {debugMode && (
                  <button
                    className={`fr-tab ${rightTab === "debug" ? "fr-tab-active" : ""}`}
                    onClick={() => setRightTab("debug")}
                  >
                    SSE Debug <span className="fr-tab-count">{debugEvents.length}</span>
                  </button>
                )}
                <span className="fr-tab-spacer" />
                {rightTab === "debug" && (
                  <button className="fr-preview-close" onClick={onClearDebug} title="Clear events">Clear</button>
                )}
                <button className="fr-preview-close" onClick={() => setPreviewOpen(false)} title="Collapse panel">◨</button>
              </div>

              {rightTab === "artifacts" && hasArtifacts && (
                <FilePreviewPanel
                  fileOps={fileOps}
                  plans={plans}
                  messages={messages}
                  selectedId={selectedFileId}
                  onSelect={setSelectedFileId}
                />
              )}

              {rightTab === "debug" && debugMode && (
                <div className="fr-debug-scroll" ref={debugScrollRef}>
                  {debugEvents.length === 0 && (
                    <div className="fr-debug-empty">No events yet. Send a message to see raw SSE events.</div>
                  )}
                  {debugEvents.map((ev, i) => (
                    <DebugEventRow key={i} ev={ev} />
                  ))}
                </div>
              )}
            </div>
          </>
        )}

        {showRightPanel && !previewOpen && (
          <div className="fr-preview-collapsed" onClick={() => setPreviewOpen(true)}>
            <span className="fr-preview-collapsed-icon">◧</span>
            <span className="fr-preview-collapsed-label">
              {rightTab === "debug" ? "Debug" : `Artifacts (${fileOps.length + plans.length})`}
            </span>
          </div>
        )}
      </div>
      </FilePreviewContext.Provider>
    </AssistantRuntimeProvider>
  );
}
