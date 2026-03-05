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
import type { MultiRepoSnapshot } from "./FilePreviewContext";
import { extractLatestTodos } from "./chatUtils";
import StickyTodoPanel from "./StickyTodoPanel";
import type { ImageAttachment, DebugEvent, PendingPermission } from "./useAgentChat";

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

/* ── Slash command registry ──────────────────────────────────────── */

const SLASH_COMMANDS = [
  { command: "/compact", description: "Compress conversation context", type: "backend" as const },
  { command: "/clear", description: "Clear chat history", type: "local" as const },
  { command: "/help", description: "Show available commands", type: "local" as const },
];

/* ── Permission Banner ─────────────────────────────────────────── */

function PermissionBanner({
  permissions,
  onRespond,
}: {
  permissions: PendingPermission[];
  onRespond: (requestId: string, decision: "allow" | "deny") => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  if (permissions.length === 0) return null;

  return (
    <div className="fr-permission-banner">
      {permissions.map((p) => (
        <div key={p.requestId} className="fr-permission-item">
          <div className="fr-permission-header">
            <span className="fr-permission-icon">🔐</span>
            <span className="fr-permission-tool">{p.toolName}</span>
            <span className="fr-permission-label">wants permission</span>
            <button
              className="fr-permission-toggle"
              onClick={() => setExpandedId(expandedId === p.requestId ? null : p.requestId)}
            >
              {expandedId === p.requestId ? "▾" : "▸"}
            </button>
            <span className="fr-permission-spacer" />
            <button
              className="fr-permission-btn fr-permission-allow"
              onClick={() => onRespond(p.requestId, "allow")}
            >
              Allow
            </button>
            <button
              className="fr-permission-btn fr-permission-deny"
              onClick={() => onRespond(p.requestId, "deny")}
            >
              Deny
            </button>
          </div>
          {expandedId === p.requestId && (
            <pre className="fr-permission-input">
              {JSON.stringify(p.toolInput, null, 2)}
            </pre>
          )}
        </div>
      ))}
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
  onClear: () => void;
  onInjectAssistant: (text: string) => void;
  attachments: ImageAttachment[];
  onAddFiles: (files: FileList | File[]) => void;
  onRemoveAttachment: (id: string) => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
  debugMode: boolean;
  debugEvents: DebugEvent[];
  onToggleDebug: () => void;
  onClearDebug: () => void;
  gitSnapshot: MultiRepoSnapshot | null;
  pendingPermissions: PendingPermission[];
  onPermissionResponse: (requestId: string, decision: "allow" | "deny") => void;
}

export default function AgentChatThread({
  messages,
  isStreaming,
  resultMeta,
  isDone,
  onNew,
  onCancel,
  onClear,
  onInjectAssistant,
  attachments,
  onAddFiles,
  onRemoveAttachment,
  fileInputRef,
  debugMode,
  debugEvents,
  onToggleDebug,
  onClearDebug,
  gitSnapshot,
  pendingPermissions,
  onPermissionResponse,
}: AgentChatThreadProps) {
  const [dragOver, setDragOver] = useState(false);

  /* ── Slash command state ── */
  const [slashFilter, setSlashFilter] = useState<string | null>(null);
  const [slashIndex, setSlashIndex] = useState(0);

  const filteredCommands = useMemo(() => {
    if (slashFilter === null) return [];
    if (slashFilter === "") return SLASH_COMMANDS;
    const q = slashFilter.toLowerCase();
    return SLASH_COMMANDS.filter((c) => c.command.slice(1).startsWith(q));
  }, [slashFilter]);

  // Reset selection when filter changes
  const prevFilterRef = useRef(slashFilter);
  if (slashFilter !== prevFilterRef.current) {
    prevFilterRef.current = slashFilter;
    setSlashIndex(0);
  }

  const handleComposerChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value;
    if (val.startsWith("/")) {
      setSlashFilter(val.slice(1));
    } else {
      setSlashFilter(null);
    }
  }, []);

  const composerInputRef = useRef<HTMLTextAreaElement>(null);

  const selectSlashCommand = useCallback((cmd: string) => {
    setSlashFilter(null);
    // Fill the composer with the command — we need to set the native input value
    const input = composerInputRef.current;
    if (input) {
      const nativeSetter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
      nativeSetter?.call(input, cmd);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.focus();
    }
  }, []);

  const handleSlashKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (slashFilter === null || filteredCommands.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSlashIndex((i) => (i + 1) % filteredCommands.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSlashIndex((i) => (i - 1 + filteredCommands.length) % filteredCommands.length);
    } else if (e.key === "Tab" || (e.key === "Enter" && filteredCommands.length > 0)) {
      // Tab always selects; Enter selects only if popup is showing
      // But if the full command is already typed and user presses Enter, let it send
      const input = composerInputRef.current;
      const currentVal = input?.value || "";
      const isExactMatch = SLASH_COMMANDS.some((c) => c.command === currentVal.trim());
      if (e.key === "Tab" || !isExactMatch) {
        e.preventDefault();
        selectSlashCommand(filteredCommands[slashIndex].command);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      setSlashFilter(null);
    }
  }, [slashFilter, filteredCommands, slashIndex, selectSlashCommand]);

  // Stable references prevent useExternalStoreRuntime from flushing its
  // internal converter cache on every render — without this, the runtime
  // re-converts ALL messages each frame, which can delay tool-call rendering.
  const convertMessage = useCallback((msg: ThreadMessageLike) => msg, []);

  const handleNew = useCallback(
    async (message: { content: unknown; role?: string }) => {
      // Extract text from message content
      let text = "";
      const content = message.content;
      if (typeof content === "string") {
        text = content;
      } else if (Array.isArray(content)) {
        text = (content as Array<Record<string, unknown>>)
          .filter((p) => p.type === "text" && p.text)
          .map((p) => p.text as string)
          .join("\n");
      }

      const trimmed = text.trim();

      // Check for slash commands
      if (trimmed.startsWith("/")) {
        const cmd = SLASH_COMMANDS.find((c) => c.command === trimmed);
        setSlashFilter(null);

        if (cmd?.type === "local") {
          if (cmd.command === "/clear") {
            onClear();
            return;
          }
          if (cmd.command === "/help") {
            const helpText = "**Available commands:**\n" + SLASH_COMMANDS.map(
              (c) => `\`${c.command}\` — ${c.description}`
            ).join("\n");
            onInjectAssistant(helpText);
            return;
          }
        }

        // Backend command (e.g. /compact) or unknown — forward as-is
        await onNew(message);
        return;
      }

      await onNew(message);
    },
    [onNew, onClear, onInjectAssistant],
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
  const runtime = useExternalStoreRuntime({
    isRunning: isStreaming,
    messages,
    convertMessage,
    onNew: handleNew,
    onCancel: handleCancel,
    onAddToolResult: handleAddToolResult,
  });

  const handleFileSelect = useCallback((_toolCallId: string) => {
    // File preview now handled externally
  }, []);

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
        className={`fr-wrap ${dragOver ? "fr-drag-over" : ""}`}
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

          {isStreaming && pendingPermissions.length === 0 && (
            <div className="fr-busy">
              <span className="fr-busy-dot" />
              <span>Thinking…</span>
            </div>
          )}

          <PermissionBanner
            permissions={pendingPermissions}
            onRespond={onPermissionResponse}
          />

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
            {slashFilter !== null && filteredCommands.length > 0 && (
              <div className="ac-slash-popup">
                {filteredCommands.map((cmd, i) => (
                  <div
                    key={cmd.command}
                    className={`ac-slash-item ${i === slashIndex ? "ac-slash-item-active" : ""}`}
                    onMouseDown={(e) => {
                      e.preventDefault();
                      selectSlashCommand(cmd.command);
                    }}
                    onMouseEnter={() => setSlashIndex(i)}
                  >
                    <span className="ac-slash-cmd">{cmd.command}</span>
                    <span className="ac-slash-desc">{cmd.description}</span>
                  </div>
                ))}
              </div>
            )}
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
                ref={composerInputRef}
                rows={1}
                placeholder="Send a message..."
                autoFocus
                onChange={handleComposerChange}
                onKeyDown={handleSlashKeyDown}
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

      </div>
      </FilePreviewContext.Provider>
    </AssistantRuntimeProvider>
  );
}
