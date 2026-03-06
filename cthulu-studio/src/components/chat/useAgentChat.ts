import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { startAgentChat, reconnectAgentChat } from "../../api/interactStream";
import { stopAgentChat, getSessionStatus, getSessionLog, getGitSnapshot } from "../../api/client";
import { replayLogLines, type ContentPart, type ToolCallPart } from "./chatParser";
import { fileToBase64 } from "./chatUtils";
import type { MultiRepoSnapshot } from "./FilePreviewContext";

export interface DebugEvent {
  ts: number;
  type: string;
  data: string;
  error?: boolean;
}

const MAX_DEBUG_EVENTS = 50;

export interface ImageAttachment {
  id: string;
  file: File;
  preview: string; // object URL for thumbnail
  media_type: string;
  data?: string; // base64 data, filled on send
}

interface SSEEvent {
  type: string;
  data: string;
}

/**
 * Apply a single SSE event to the mutable partsRef.
 * Returns true if React state should be flushed immediately (non-text events).
 */
function applySSEEvent(
  event: SSEEvent,
  partsRef: React.MutableRefObject<ContentPart[]>,
  setResultMeta: (meta: { cost: number; turns: number }) => void,
  setGitSnapshot?: (snapshot: MultiRepoSnapshot) => void,
): boolean {
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
      return false; // batched via rAF
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
      return true;
    } else if (event.type === "tool_result") {
      const parts = partsRef.current;
      for (let i = parts.length - 1; i >= 0; i--) {
        if (parts[i].type === "tool-call" && !(parts[i] as ToolCallPart).result) {
          const updated = [...parts];
          updated[i] = { ...(parts[i] as ToolCallPart), result: data.content ?? data.output ?? "done" };
          partsRef.current = updated;
          return true;
        }
      }
    } else if (event.type === "git_snapshot") {
      if (setGitSnapshot) {
        setGitSnapshot(data as MultiRepoSnapshot);
      }
      return false;
    } else if (event.type === "result") {
      const hasText = partsRef.current.some((p) => p.type === "text");
      if (data.text && !hasText) {
        partsRef.current = [...partsRef.current, { type: "text", text: data.text }];
      }
      setResultMeta({ cost: data.cost || 0, turns: data.turns || 0 });
      return true;
    }
  } catch {
    // Fallback: treat as raw text
    if (event.type === "text") {
      const parts = partsRef.current;
      const last = parts[parts.length - 1];
      if (last && last.type === "text") {
        last.text += event.data;
      } else {
        parts.push({ type: "text", text: event.data });
      }
      return true;
    }
  }
  return false;
}

export function useAgentChat(agentId: string, sessionId: string) {
  const [messages, setMessages] = useState<ThreadMessageLike[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamingParts, setStreamingParts] = useState<ContentPart[]>([]);
  const [resultMeta, setResultMeta] = useState<{ cost: number; turns: number } | null>(null);
  const [isDone, setIsDone] = useState(false);
  const [attachments, setAttachments] = useState<ImageAttachment[]>([]);
  const abortRef = useRef<AbortController | null>(null);
  const rafRef = useRef<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Mutable mirror of streamingParts — SSE callbacks read/write this,
  // then flush to React state via rAF or direct setState.
  const partsRef = useRef<ContentPart[]>([]);

  // Git snapshot state — updated via SSE events and initial fetch
  const [gitSnapshot, setGitSnapshot] = useState<MultiRepoSnapshot | null>(null);

  // File changes tracked via PostToolUse hooks (received from global hook stream)
  const [changedFiles, setChangedFiles] = useState<string[]>([]);

  // Debug mode: capture raw SSE events for inspection
  const [debugMode, setDebugMode] = useState(false);
  const [debugEvents, setDebugEvents] = useState<DebugEvent[]>([]);
  const debugEventsRef = useRef<DebugEvent[]>([]);
  const debugModeRef = useRef(false);
  debugModeRef.current = debugMode;

  const pushDebugEvent = useCallback((event: SSEEvent, error?: boolean) => {
    const entry: DebugEvent = { ts: Date.now(), type: event.type, data: event.data, error };
    const buf = debugEventsRef.current;
    buf.push(entry);
    if (buf.length > MAX_DEBUG_EVENTS) buf.shift();
    if (debugModeRef.current) {
      setDebugEvents([...buf]);
    }
  }, []);

  const clearDebugEvents = useCallback(() => {
    debugEventsRef.current = [];
    setDebugEvents([]);
  }, []);

  // When debug mode is toggled on, flush any buffered events
  useEffect(() => {
    if (debugMode && debugEventsRef.current.length > 0) {
      setDebugEvents([...debugEventsRef.current]);
    }
  }, [debugMode]);

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
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Cleanup on unmount: abort any in-flight stream and cancel pending rAF.
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
    // Fetch initial git snapshot
    getGitSnapshot(agentId, sessionId)
      .then((snap) => {
        if (!cancelled && snap) setGitSnapshot(snap);
      })
      .catch(() => { /* no git integration */ });
    return () => { cancelled = true; };
  }, [agentId, sessionId]);

  const flushParts = useCallback(() => {
    rafRef.current = null;
    setStreamingParts([...partsRef.current]);
  }, []);

  const scheduleFlush = useCallback(() => {
    if (rafRef.current == null) {
      rafRef.current = requestAnimationFrame(flushParts);
    }
  }, [flushParts]);

  const finalizeParts = useCallback(() => {
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
    setIsDone(true);
  }, []);

  const resetStreaming = useCallback(() => {
    setStreamingParts([]);
    partsRef.current = [];
    setIsStreaming(false);
  }, []);

  // Auto-reconnect: on mount, check session status. If busy, reconnect SSE.
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const status = await getSessionStatus(agentId, sessionId);
        if (cancelled || !status.busy) return;

        setIsStreaming(true);
        setStreamingParts([]);
        partsRef.current = [];

        const controller = reconnectAgentChat(
          agentId,
          sessionId,
          (event) => {
            if (cancelled) return;
            pushDebugEvent(event);
            const needsFlush = applySSEEvent(event, partsRef, setResultMeta, setGitSnapshot);
            if (needsFlush) {
              setStreamingParts([...partsRef.current]);
            } else {
              scheduleFlush();
            }
          },
          () => {
            pushDebugEvent({ type: "done", data: "{}" });
            finalizeParts();
          },
          (err) => {
            pushDebugEvent({ type: "error", data: String(err) }, true);
            resetStreaming();
          },
        );

        abortRef.current = controller;
      } catch {
        // Failed to check session status
      }
    })();

    return () => { cancelled = true; };
  }, [agentId, sessionId, scheduleFlush, finalizeParts, resetStreaming]);

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
        attachments.forEach((a) => URL.revokeObjectURL(a.preview));
        setAttachments([]);
      }

      const userContent: { type: string; text?: string; image?: string }[] = [{ type: "text", text }];
      if (images) {
        images.forEach((img) => userContent.push({ type: "image", image: `data:${img.media_type};base64,${img.data}` }));
      }
      setMessages((prev) => [...prev, { role: "user" as const, content: userContent as ThreadMessageLike["content"], createdAt: new Date() }]);
      setIsStreaming(true);
      setIsDone(false);
      setResultMeta(null);
      setStreamingParts([]);
      partsRef.current = [];

      const controller = startAgentChat(
        agentId,
        text,
        sessionId,
        (event) => {
          pushDebugEvent(event);
          const needsFlush = applySSEEvent(event, partsRef, setResultMeta, setGitSnapshot);
          if (needsFlush) {
            setStreamingParts([...partsRef.current]);
          } else {
            scheduleFlush();
          }
        },
        () => {
          pushDebugEvent({ type: "done", data: "{}" });
          finalizeParts();
        },
        (err) => {
          pushDebugEvent({ type: "error", data: String(err) }, true);
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
          resetStreaming();
        },
        undefined, // flowContext
        images,
      );

      abortRef.current = controller;
    },
    [agentId, sessionId, attachments, scheduleFlush, finalizeParts, resetStreaming, pushDebugEvent]
  );

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    stopAgentChat(agentId, sessionId).catch(() => {});
  }, [agentId, sessionId]);

  const clearMessages = useCallback(() => {
    setMessages([]);
    setStreamingParts([]);
    partsRef.current = [];
    setIsStreaming(false);
    setIsDone(false);
    setResultMeta(null);
  }, []);

  const injectAssistantMessage = useCallback((text: string) => {
    setMessages((prev) => [
      ...prev,
      { role: "assistant" as const, content: [{ type: "text" as const, text }], createdAt: new Date() },
    ]);
  }, []);

  const threadMessages = useMemo((): ThreadMessageLike[] => {
    if (!isStreaming) return messages;
    return streamingParts.length > 0
      ? [...messages, { role: "assistant" as const, content: streamingParts }]
      : messages;
  }, [messages, isStreaming, streamingParts]);

  return {
    messages: threadMessages,
    isStreaming,
    resultMeta,
    isDone,
    attachments,
    fileInputRef,
    handleSend,
    handleCancel,
    clearMessages,
    injectAssistantMessage,
    addFiles,
    removeAttachment,
    debugMode,
    setDebugMode,
    debugEvents,
    clearDebugEvents,
    gitSnapshot,
    changedFiles,
  };
}
