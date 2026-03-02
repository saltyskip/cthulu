import { useState, useRef, useCallback, useMemo, useEffect, memo } from "react";
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
} from "./ChatPrimitives";
import { startAgentChat, reconnectAgentChat } from "../api/interactStream";
import { stopAgentChat, getSessionStatus, getSessionLog } from "../api/client";
import { AskUserQuestionToolUI, type TodoItem } from "./ToolRenderers";
import { createContext, useContext } from "react";

// Context for tool renderers to signal "select this file in the preview panel"
type FilePreviewContextType = ((toolCallId: string) => void) | null;
const FilePreviewContext = createContext<FilePreviewContextType>(null);
export const useFilePreviewSelect = () => useContext(FilePreviewContext);

// File operation extracted from messages for the preview panel
export interface FileOp {
  toolCallId: string;
  filePath: string;
  type: "edit" | "write";
  oldString?: string;
  newString?: string;
  content?: string;
}

interface AgentChatViewProps {
  agentId: string;
  sessionId: string;
  busy?: boolean;
}

// Internal mutable type for streaming content parts.
interface TextPart { type: "text"; text: string }
interface ToolCallPart {
  type: "tool-call";
  toolCallId: string;
  toolName: string;
  args: Record<string, string | number | boolean | null>;
  result?: unknown;
}
type ContentPart = TextPart | ToolCallPart;


/** Replay JSONL log lines into ThreadMessageLike[] messages. */
function replayLogLines(lines: string[]): ThreadMessageLike[] {
  const messages: ThreadMessageLike[] = [];
  let currentParts: ContentPart[] = [];

  const flushAssistant = () => {
    if (currentParts.length > 0) {
      messages.push({ role: "assistant" as const, content: [...currentParts] });
      currentParts = [];
    }
  };

  for (const raw of lines) {
    const colonIdx = raw.indexOf(":");
    if (colonIdx === -1) continue;
    const eventType = raw.slice(0, colonIdx);
    const payload = raw.slice(colonIdx + 1);

    if (eventType === "user") {
      flushAssistant();
      try {
        const data = JSON.parse(payload);
        messages.push({ role: "user" as const, content: [{ type: "text" as const, text: data.text || "" }] });
      } catch { /* skip corrupt */ }
    } else if (eventType === "text") {
      try {
        const data = JSON.parse(payload);
        const text = data.text || "";
        // Merge consecutive text parts
        const last = currentParts[currentParts.length - 1];
        if (last && last.type === "text") {
          (last as TextPart).text += text;
        } else {
          currentParts.push({ type: "text", text });
        }
      } catch { /* skip */ }
    } else if (eventType === "tool_use") {
      try {
        const data = JSON.parse(payload);
        let parsedArgs: Record<string, string | number | boolean | null> = {};
        if (typeof data.input === "string" && data.input) {
          try { parsedArgs = JSON.parse(data.input); } catch { /* */ }
        } else if (typeof data.input === "object" && data.input) {
          parsedArgs = data.input;
        }
        currentParts.push({
          type: "tool-call" as const,
          toolCallId: data.id || `tool-replay-${currentParts.length}`,
          toolName: data.tool || data.name || "unknown",
          args: parsedArgs,
        });
      } catch { /* skip */ }
    } else if (eventType === "tool_result") {
      try {
        const data = JSON.parse(payload);
        // Find last unresolved tool-call
        for (let i = currentParts.length - 1; i >= 0; i--) {
          if (currentParts[i].type === "tool-call" && !(currentParts[i] as ToolCallPart).result) {
            (currentParts[i] as ToolCallPart).result = data.content ?? data.output ?? "done";
            break;
          }
        }
      } catch { /* skip */ }
    } else if (eventType === "result") {
      try {
        const data = JSON.parse(payload);
        const hasText = currentParts.some((p) => p.type === "text");
        if (data.text && !hasText) {
          currentParts.push({ type: "text", text: data.text });
        }
      } catch { /* skip */ }
    } else if (eventType === "done") {
      flushAssistant();
    }
    // stderr and other events are ignored for message replay
  }

  // Flush any remaining assistant parts
  flushAssistant();
  return messages;
}

export interface ImageAttachment {
  id: string;
  file: File;
  preview: string; // object URL for thumbnail
  media_type: string;
  data?: string; // base64 data, filled on send
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Strip data URL prefix: "data:image/png;base64,..."
      resolve(result.split(",")[1]);
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

export default function AgentChatView({ agentId, sessionId, busy = false }: AgentChatViewProps) {
  console.log(`[RECONNECT-DEBUG] AgentChatView RENDER agentId=${agentId} sessionId=${sessionId} busy=${busy}`);
  const [messages, setMessages] = useState<ThreadMessageLike[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamingParts, setStreamingParts] = useState<ContentPart[]>([]);
  const [resultMeta, setResultMeta] = useState<{ cost: number; turns: number } | null>(null);
  const [attachments, setAttachments] = useState<ImageAttachment[]>([]);
  const abortRef = useRef<AbortController | null>(null);
  const rafRef = useRef<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Mutable mirror of streamingParts — SSE callbacks read/write this,
  // then flush to React state via rAF or direct setState.
  const partsRef = useRef<ContentPart[]>([]);

  const addFiles = useCallback((files: FileList | File[]) => {
    const imageFiles = Array.from(files).filter((f) => f.type.startsWith("image/"));
    if (imageFiles.length === 0) return;
    const newAttachments: ImageAttachment[] = imageFiles.map((f) => ({
      id: `img-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      file: f,
      preview: URL.createObjectURL(f),
      media_type: f.type,
    }));
    setAttachments((prev) => [...prev, ...newAttachments]);
  }, []);

  const removeAttachment = useCallback((id: string) => {
    setAttachments((prev) => {
      const removed = prev.find((a) => a.id === id);
      if (removed) URL.revokeObjectURL(removed.preview);
      return prev.filter((a) => a.id !== id);
    });
  }, []);

  // Cleanup object URLs on unmount
  useEffect(() => {
    return () => {
      attachments.forEach((a) => URL.revokeObjectURL(a.preview));
    };
  }, []);

  // Cleanup on unmount: abort any in-flight stream and cancel pending rAF.
  // Prevents dangling network requests and setState-on-unmounted-component.
  useEffect(() => {
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
      abortRef.current?.abort();
    };
  }, []);

  // Restore chat history from backend JSONL log on mount.
  useEffect(() => {
    let cancelled = false;
    getSessionLog(agentId, sessionId)
      .then((lines) => {
        if (cancelled || lines.length === 0) return;
        const restored = replayLogLines(lines);
        if (restored.length > 0) {
          setMessages(restored);
        }
      })
      .catch(() => { /* backend unavailable, start empty */ });
    return () => { cancelled = true; };
  }, [agentId, sessionId]);

  // Auto-reconnect: on mount (or HMR remount), check session status directly
  // via API call. If busy, connect to the reconnect SSE endpoint to resume
  // streaming. This avoids depending on the parent's 5s polling interval.
  useEffect(() => {
    let cancelled = false;

    (async () => {
      console.log(`[RECONNECT-DEBUG] Mount check: fetching session status for ${sessionId}`);
      try {
        const status = await getSessionStatus(agentId, sessionId);
        console.log(`[RECONNECT-DEBUG] Session status: busy=${status.busy} process_alive=${status.process_alive}`);
        if (cancelled) return;
        if (!status.busy) {
          console.log("[RECONNECT-DEBUG] Session not busy, skipping reconnect");
          return;
        }

        console.log(`[RECONNECT-DEBUG] RECONNECTING to agentId=${agentId} sessionId=${sessionId}`);
        setIsStreaming(true);
        setStreamingParts([]);
        partsRef.current = [];

        const controller = reconnectAgentChat(
          agentId,
          sessionId,
          // onEvent
          (event) => {
            if (cancelled) return;
            console.log(`[RECONNECT-DEBUG] Reconnect event: ${event.type}`, event.data?.substring(0, 100));
            try {
              const data = JSON.parse(event.data);

              if (event.type === "text") {
                const parts = partsRef.current;
                const last = parts[parts.length - 1];
                if (last && last.type === "text") {
                  last.text += data.text || "";
                } else {
                  parts.push({ type: "text", text: data.text || "" });
                }
                if (rafRef.current == null) {
                  rafRef.current = requestAnimationFrame(() => {
                    rafRef.current = null;
                    setStreamingParts([...partsRef.current]);
                  });
                }
              } else if (event.type === "tool_use") {
                let parsedArgs: Record<string, string | number | boolean | null> = {};
                if (typeof data.input === "string" && data.input) {
                  try { parsedArgs = JSON.parse(data.input); } catch { /* leave empty */ }
                } else if (typeof data.input === "object" && data.input) {
                  parsedArgs = data.input;
                }
                partsRef.current = [...partsRef.current, {
                  type: "tool-call" as const,
                  toolCallId: data.id || `tool-${Date.now()}-${partsRef.current.length}`,
                  toolName: data.tool || data.name || "unknown",
                  args: parsedArgs,
                }];
                setStreamingParts([...partsRef.current]);
              } else if (event.type === "tool_result") {
                const parts = partsRef.current;
                for (let i = parts.length - 1; i >= 0; i--) {
                  if (parts[i].type === "tool-call" && !(parts[i] as ToolCallPart).result) {
                    const updated = [...parts];
                    updated[i] = { ...(parts[i] as ToolCallPart), result: data.content ?? data.output ?? "done" };
                    partsRef.current = updated;
                    setStreamingParts(updated);
                    break;
                  }
                }
              } else if (event.type === "result") {
                const hasText = partsRef.current.some((p) => p.type === "text");
                if (data.text && !hasText) {
                  partsRef.current = [...partsRef.current, { type: "text", text: data.text }];
                  setStreamingParts([...partsRef.current]);
                }
                setResultMeta({ cost: data.cost || 0, turns: data.turns || 0 });
              }
            } catch {
              if (event.type === "text") {
                const parts = partsRef.current;
                const last = parts[parts.length - 1];
                if (last && last.type === "text") {
                  last.text += event.data;
                } else {
                  parts.push({ type: "text", text: event.data });
                }
                setStreamingParts([...partsRef.current]);
              }
            }
          },
          // onDone
          () => {
            console.log(`[RECONNECT-DEBUG] Reconnect stream DONE, finalParts=${partsRef.current.length}`);
            if (rafRef.current != null) {
              cancelAnimationFrame(rafRef.current);
              rafRef.current = null;
            }
            const finalParts = partsRef.current;
            if (finalParts.length > 0) {
              setMessages((prev) => [
                ...prev,
                { role: "assistant" as const, content: [...finalParts], createdAt: new Date() },
              ]);
            }
            setStreamingParts([]);
            partsRef.current = [];
            setIsStreaming(false);
          },
          // onError
          (err) => {
            console.error(`[RECONNECT-DEBUG] Reconnect stream ERROR:`, err);
            setStreamingParts([]);
            partsRef.current = [];
            setIsStreaming(false);
          }
        );

        abortRef.current = controller;
      } catch (e) {
        console.error("[RECONNECT-DEBUG] Failed to check session status:", e);
      }
    })();

    return () => { cancelled = true; };
  }, [agentId, sessionId]);

  const flushParts = useCallback(() => {
    rafRef.current = null;
    setStreamingParts([...partsRef.current]);
  }, []);

  // Append text to the last text part, or create a new one.
  // Batches via rAF for perf during rapid token streaming.
  const appendText = useCallback((text: string) => {
    const parts = partsRef.current;
    const last = parts[parts.length - 1];
    if (last && last.type === "text") {
      last.text += text;
    } else {
      parts.push({ type: "text", text });
    }
    if (rafRef.current == null) {
      rafRef.current = requestAnimationFrame(flushParts);
    }
  }, [flushParts]);

  const handleSend = useCallback(
    async (message: { content: unknown; role?: string }) => {
      let text = "";
      const content = message.content;
      if (typeof content === "string") {
        text = content;
      } else if (Array.isArray(content)) {
        text = content
          .filter((p: Record<string, unknown>) => p.type === "text" && p.text)
          .map((p: Record<string, unknown>) => p.text as string)
          .join("\n");
      }

      if (!text.trim() && attachments.length === 0) return;

      // Convert attachments to base64
      let images: { media_type: string; data: string }[] | undefined;
      if (attachments.length > 0) {
        images = await Promise.all(
          attachments.map(async (a) => ({
            media_type: a.media_type,
            data: await fileToBase64(a.file),
          }))
        );
        // Clean up previews and clear attachments
        attachments.forEach((a) => URL.revokeObjectURL(a.preview));
        setAttachments([]);
      }

      const userContent: (Record<string, unknown>)[] = [{ type: "text", text }];
      if (images) {
        images.forEach((img) => userContent.push({ type: "image", image: `data:${img.media_type};base64,${img.data}` }));
      }
      setMessages((prev) => [...prev, { role: "user" as const, content: userContent, createdAt: new Date() }]);
      setIsStreaming(true);
      setStreamingParts([]);
      partsRef.current = [];

      const controller = startAgentChat(
        agentId,
        text,
        sessionId,
        // onEvent
        (event) => {
          console.log(`[SSE] ${event.type}:`, event.data);
          try {
            const data = JSON.parse(event.data);

            if (event.type === "text") {
              appendText(data.text || "");
            } else if (event.type === "tool_use") {
              // Backend sends {tool, input} — input may be a JSON string
              let parsedArgs: Record<string, string | number | boolean | null> = {};
              if (typeof data.input === "string" && data.input) {
                try { parsedArgs = JSON.parse(data.input); } catch { /* leave empty */ }
              } else if (typeof data.input === "object" && data.input) {
                parsedArgs = data.input;
              }
              const toolName = data.tool || data.name || "unknown";
              partsRef.current = [...partsRef.current, {
                type: "tool-call" as const,
                toolCallId: data.id || `tool-${Date.now()}-${partsRef.current.length}`,
                toolName,
                args: parsedArgs,
              }];
              setStreamingParts([...partsRef.current]);
            } else if (event.type === "tool_result") {
              const parts = partsRef.current;
              // Find last tool-call part (scanning backwards)
              for (let i = parts.length - 1; i >= 0; i--) {
                if (parts[i].type === "tool-call" && !(parts[i] as ToolCallPart).result) {
                  const updated = [...parts];
                  updated[i] = {
                    ...(parts[i] as ToolCallPart),
                    result: data.content ?? data.output ?? "done",
                  };
                  partsRef.current = updated;
                  setStreamingParts(updated);
                  break;
                }
              }
            } else if (event.type === "result") {
              // Result event carries the complete response text AND cost/turns.
              // Only use result.text if no text was streamed.
              const hasText = partsRef.current.some((p) => p.type === "text");
              if (data.text && !hasText) {
                partsRef.current = [...partsRef.current, { type: "text", text: data.text }];
                setStreamingParts([...partsRef.current]);
              }
              setResultMeta({
                cost: data.cost || 0,
                turns: data.turns || 0,
              });
            }
          } catch {
            if (event.type === "text") {
              appendText(event.data);
            }
          }
        },
        // onDone
        () => {
          if (rafRef.current != null) {
            cancelAnimationFrame(rafRef.current);
            rafRef.current = null;
          }

          const finalParts = partsRef.current;
          if (finalParts.length > 0) {
            setMessages((prev) => [
              ...prev,
              { role: "assistant" as const, content: [...finalParts], createdAt: new Date() },
            ]);
          }

          setStreamingParts([]);
          partsRef.current = [];
          setIsStreaming(false);
        },
        // onError
        (err) => {
          console.error("Agent chat error:", err);
          // Map HTTP error codes to actionable messages
          let errorMsg = String(err);
          if (errorMsg.includes("409")) {
            errorMsg = "Session is busy processing a previous message. Wait for it to finish or force-kill it.";
          } else if (errorMsg.includes("429")) {
            errorMsg = "Session limit reached. Close an existing session before creating a new one.";
          }
          setMessages((prev) => [
            ...prev,
            { role: "assistant" as const, content: `Error: ${errorMsg}`, createdAt: new Date() },
          ]);
          setStreamingParts([]);
          partsRef.current = [];
          setIsStreaming(false);
        },
        undefined, // flowContext
        images,
      );

      abortRef.current = controller;
    },
    [agentId, sessionId, appendText, attachments]
  );

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    stopAgentChat(agentId, sessionId).catch(() => {});
  }, [agentId, sessionId]);

  const threadMessages = useMemo((): ThreadMessageLike[] => {
    if (!isStreaming) return messages;
    return streamingParts.length > 0
      ? [...messages, { role: "assistant" as const, content: streamingParts }]
      : messages;
  }, [messages, isStreaming, streamingParts]);

  return (
    <AgentChatThread
      messages={threadMessages}
      isStreaming={isStreaming}
      resultMeta={resultMeta}
      onNew={handleSend}
      onCancel={handleCancel}
      attachments={attachments}
      onAddFiles={addFiles}
      onRemoveAttachment={removeAttachment}
      fileInputRef={fileInputRef}
    />
  );
}

/** Extract all Edit/Write file operations from messages. */
function extractFileOps(messages: ThreadMessageLike[]): FileOp[] {
  const ops: FileOp[] = [];
  for (const msg of messages) {
    const content = msg.content;
    if (!Array.isArray(content)) continue;
    for (const part of content) {
      const p = part as Record<string, unknown>;
      if (p.type !== "tool-call") continue;
      const args = p.args as Record<string, unknown> | undefined;
      if (!args) continue;
      const toolCallId = (p.toolCallId as string) || "";
      if (p.toolName === "Edit" && args.file_path) {
        ops.push({
          toolCallId,
          filePath: args.file_path as string,
          type: "edit",
          oldString: args.old_string as string | undefined,
          newString: args.new_string as string | undefined,
        });
      } else if (p.toolName === "Write" && args.file_path) {
        ops.push({
          toolCallId,
          filePath: args.file_path as string,
          type: "write",
          content: args.content as string | undefined,
        });
      }
    }
  }
  return ops;
}

function basename(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  return parts.pop() || filePath;
}

function computeDiffLines(oldStr: string, newStr: string) {
  const oldLines = oldStr.split("\n");
  const newLines = newStr.split("\n");
  const m = oldLines.length;
  const n = newLines.length;

  if (m + n > 500) {
    const lines: { type: "del" | "add" | "ctx"; text: string }[] = [];
    oldLines.forEach((l) => lines.push({ type: "del", text: l }));
    newLines.forEach((l) => lines.push({ type: "add", text: l }));
    return lines;
  }

  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] = oldLines[i - 1] === newLines[j - 1]
        ? dp[i - 1][j - 1] + 1
        : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }

  const result: { type: "del" | "add" | "ctx"; text: string }[] = [];
  let i = m, j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
      result.push({ type: "ctx", text: oldLines[i - 1] });
      i--; j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      result.push({ type: "add", text: newLines[j - 1] });
      j--;
    } else {
      result.push({ type: "del", text: oldLines[i - 1] });
      i--;
    }
  }
  result.reverse();
  return result;
}

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  selectedId,
  onSelect,
  onClose,
  width,
}: {
  fileOps: FileOp[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onClose: () => void;
  width: number;
}) {
  const op = selectedId ? fileOps.find((f) => f.toolCallId === selectedId) : fileOps[fileOps.length - 1];
  if (!op) return null;

  const diffLines = op.type === "edit" && op.oldString !== undefined && op.newString !== undefined
    ? computeDiffLines(op.oldString, op.newString)
    : null;

  // Build tree structure grouped by directory
  const fileMap = new Map<string, FileOp>();
  for (const f of fileOps) fileMap.set(f.filePath, f);
  const uniqueFiles = [...fileMap.values()];

  // Group by parent directory
  const groups = new Map<string, FileOp[]>();
  for (const f of uniqueFiles) {
    const parts = f.filePath.replace(/\\/g, "/").split("/");
    const name = parts.pop() || f.filePath;
    const dir = parts.length > 0 ? parts.slice(-2).join("/") : "";
    const existing = groups.get(dir) || [];
    existing.push({ ...f, filePath: name }); // store basename for display
    groups.set(dir, existing);
  }

  return (
    <div className="fr-preview" style={{ width, flex: `0 0 ${width}px` }}>
      <div className="fr-preview-topbar">
        <span className="fr-preview-title">Files</span>
        <span className="fr-preview-count">{uniqueFiles.length}</span>
        <button className="fr-preview-close" onClick={onClose} title="Collapse panel">◨</button>
      </div>
      <div className="fr-preview-split">
        <div className="fr-preview-tree">
          {[...groups.entries()].map(([dir, files]) => (
            <div key={dir} className="fr-tree-group">
              {dir && <div className="fr-tree-dir">{dir}</div>}
              {files.map((f) => {
                // Find the original toolCallId from fileMap
                const original = uniqueFiles.find((o) => o.filePath.endsWith(f.filePath) && o.toolCallId === f.toolCallId);
                const isActive = original?.toolCallId === op.toolCallId;
                return (
                  <button
                    key={f.toolCallId}
                    className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                    onClick={() => onSelect(f.toolCallId)}
                  >
                    <span className="fr-tree-icon">{f.type === "edit" ? "✎" : "📄"}</span>
                    {f.filePath}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
        <div className="fr-preview-main">
          <div className="fr-preview-path">{op.filePath}</div>
          <div className="fr-preview-body">
            {diffLines ? (
              <div className="fr-preview-diff">
                {diffLines.map((line, i) => (
                  <div
                    key={i}
                    className={`fr-diff-line ${
                      line.type === "del" ? "fr-diff-del" : line.type === "add" ? "fr-diff-add" : "fr-diff-ctx"
                    }`}
                  >
                    <span className="fr-diff-prefix">
                      {line.type === "del" ? "−" : line.type === "add" ? "+" : " "}
                    </span>
                    {line.text}
                  </div>
                ))}
              </div>
            ) : op.content ? (
              <pre className="fr-preview-content">{op.content}</pre>
            ) : (
              <div className="fr-preview-empty">No preview available</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
});

/** Scan messages backwards for the most recent TodoWrite tool call and extract its todos. */
function extractLatestTodos(messages: ThreadMessageLike[]): TodoItem[] | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i];
    const content = msg.content;
    if (!Array.isArray(content)) continue;
    for (let j = content.length - 1; j >= 0; j--) {
      const part = content[j] as Record<string, unknown>;
      if (part.type === "tool-call" && part.toolName === "TodoWrite") {
        const args = part.args as Record<string, unknown> | undefined;
        if (args?.todos && Array.isArray(args.todos)) {
          return args.todos as TodoItem[];
        }
      }
    }
  }
  return null;
}

const StickyTodoPanel = memo(function StickyTodoPanel({ todos }: { todos: TodoItem[] }) {
  const [collapsed, setCollapsed] = useState(false);
  const completed = todos.filter((t) => t.status === "completed").length;
  const total = todos.length;
  const pct = total > 0 ? Math.round((completed / total) * 100) : 0;

  return (
    <div className="fr-sticky-todo">
      <div className="fr-sticky-todo-header" onClick={() => setCollapsed((v) => !v)}>
        <span className="fr-sticky-todo-caret">{collapsed ? "▸" : "▾"}</span>
        <span className="fr-sticky-todo-title">Tasks</span>
        <span className="fr-sticky-todo-progress">{completed}/{total}</span>
        <div className="fr-sticky-todo-bar">
          <div className="fr-sticky-todo-fill" style={{ width: `${pct}%` }} />
        </div>
      </div>
      {!collapsed && (
        <div className="fr-sticky-todo-list">
          {todos.map((t, i) => (
            <div key={i} className={`fr-todo-item fr-todo-${t.status.replace("_", "-")}`}>
              <span className="fr-todo-check">
                {t.status === "completed" ? "✓" : t.status === "in_progress" ? "●" : "○"}
              </span>
              <span className="fr-todo-text">
                {t.status === "in_progress" && t.activeForm ? t.activeForm : t.content}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
});

function AgentChatThread({
  messages,
  isStreaming,
  resultMeta,
  onNew,
  onCancel,
  attachments,
  onAddFiles,
  onRemoveAttachment,
  fileInputRef,
}: {
  messages: ThreadMessageLike[];
  isStreaming: boolean;
  resultMeta: { cost: number; turns: number } | null;
  onNew: (message: { content: unknown; role?: string }) => Promise<void>;
  onCancel: () => void;
  attachments: ImageAttachment[];
  onAddFiles: (files: FileList | File[]) => void;
  onRemoveAttachment: (id: string) => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
}) {
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

  const latestTodos = useMemo(() => extractLatestTodos(messages), [messages]);
  const fileOps = useMemo(() => extractFileOps(messages), [messages]);
  const [selectedFileId, setSelectedFileId] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [previewWidth, setPreviewWidth] = useState(480);
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);

  // Auto-select latest file op as new ones arrive
  const prevOpsLenRef = useRef(0);
  useEffect(() => {
    if (fileOps.length > prevOpsLenRef.current) {
      setSelectedFileId(fileOps[fileOps.length - 1].toolCallId);
      if (!previewOpen && fileOps.length > 0) setPreviewOpen(true);
    }
    prevOpsLenRef.current = fileOps.length;
  }, [fileOps, previewOpen]);

  const handleDividerDrag = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current = { startX: e.clientX, startW: previewWidth };

    const onMove = (ev: MouseEvent) => {
      if (!dragRef.current) return;
      // Dragging left = wider preview (preview is on right)
      const delta = dragRef.current.startX - ev.clientX;
      const maxW = Math.max(240, window.innerWidth - 300); // leave at least 300px for chat
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

  const hasFiles = fileOps.length > 0;

  const handleFileSelect = useCallback((toolCallId: string) => {
    setSelectedFileId(toolCallId);
    if (!previewOpen) setPreviewOpen(true);
  }, [previewOpen]);

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
        className={`fr-wrap ${hasFiles ? "fr-wrap-split" : ""} ${dragOver ? "fr-drag-over" : ""}`}
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

          {resultMeta && !isStreaming && (
            <div className="fr-result">
              {resultMeta.turns}t &middot; ${resultMeta.cost.toFixed(4)}
            </div>
          )}
        </div>

        {hasFiles && previewOpen && (
          <>
            <div className="fr-preview-divider" onMouseDown={handleDividerDrag} />
            <FilePreviewPanel
              fileOps={fileOps}
              selectedId={selectedFileId}
              onSelect={setSelectedFileId}
              onClose={() => setPreviewOpen(false)}
              width={previewWidth}
            />
          </>
        )}

        {hasFiles && !previewOpen && (
          <div className="fr-preview-collapsed" onClick={() => setPreviewOpen(true)}>
            <span className="fr-preview-collapsed-icon">◧</span>
            <span className="fr-preview-collapsed-label">Files ({fileOps.length})</span>
          </div>
        )}
      </div>
      </FilePreviewContext.Provider>
    </AssistantRuntimeProvider>
  );
}
