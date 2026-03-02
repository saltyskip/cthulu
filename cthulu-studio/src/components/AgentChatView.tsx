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
} from "./ChatPrimitives";
import { startAgentChat } from "../api/interactStream";
import { stopAgentChat } from "../api/client";
import { AskUserQuestionToolUI } from "./ToolRenderers";

interface AgentChatViewProps {
  agentId: string;
  sessionId: string;
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

export default function AgentChatView({ agentId, sessionId }: AgentChatViewProps) {
  const [messages, setMessages] = useState<ThreadMessageLike[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamingParts, setStreamingParts] = useState<ContentPart[]>([]);
  const [resultMeta, setResultMeta] = useState<{ cost: number; turns: number } | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const rafRef = useRef<number | null>(null);
  // Mutable mirror of streamingParts — SSE callbacks read/write this,
  // then flush to React state via rAF or direct setState.
  const partsRef = useRef<ContentPart[]>([]);

  // Cleanup on unmount: abort any in-flight stream and cancel pending rAF.
  // Prevents dangling network requests and setState-on-unmounted-component.
  useEffect(() => {
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
      abortRef.current?.abort();
    };
  }, []);

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

      if (!text.trim()) return;

      setMessages((prev) => [...prev, { role: "user" as const, content: text }]);
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
              { role: "assistant" as const, content: [...finalParts] },
            ]);
          }

          setStreamingParts([]);
          partsRef.current = [];
          setIsStreaming(false);
        },
        // onError
        (err) => {
          console.error("Agent chat error:", err);
          setMessages((prev) => [
            ...prev,
            { role: "assistant" as const, content: `Error: ${err}` },
          ]);
          setStreamingParts([]);
          partsRef.current = [];
          setIsStreaming(false);
        }
      );

      abortRef.current = controller;
    },
    [agentId, sessionId, appendText]
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
    />
  );
}

function AgentChatThread({
  messages,
  isStreaming,
  resultMeta,
  onNew,
  onCancel,
}: {
  messages: ThreadMessageLike[];
  isStreaming: boolean;
  resultMeta: { cost: number; turns: number } | null;
  onNew: (message: { content: unknown; role?: string }) => Promise<void>;
  onCancel: () => void;
}) {
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

  const runtime = useExternalStoreRuntime({
    isRunning: isStreaming,
    messages,
    convertMessage,
    onNew: handleNew,
    onCancel: handleCancel,
    onAddToolResult: handleAddToolResult,
  });

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <AskUserQuestionToolUI />
      <div className="fr-wrap">
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

        <div className="ac-footer">
          <ComposerPrimitive.Root>
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
    </AssistantRuntimeProvider>
  );
}
