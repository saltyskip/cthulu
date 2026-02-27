import { useState, useRef, useCallback, useMemo } from "react";
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
  const [streamingText, setStreamingText] = useState("");
  const [streamingToolCalls, setStreamingToolCalls] = useState<ContentPart[]>([]);
  const [resultMeta, setResultMeta] = useState<{ cost: number; turns: number } | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const rafRef = useRef<number | null>(null);
  const textBufRef = useRef("");
  // Mirror of streamingToolCalls for reading in onDone without nested setState
  const toolCallsRef = useRef<ContentPart[]>([]);

  const flushText = useCallback(() => {
    rafRef.current = null;
    setStreamingText(textBufRef.current);
  }, []);

  const handleSend = useCallback(
    async (message: { content: unknown; role?: string }) => {
      // Extract text from the AppendMessage content parts
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

      // Append user message
      setMessages((prev) => [...prev, { role: "user" as const, content: text }]);
      setIsStreaming(true);
      setStreamingText("");
      setStreamingToolCalls([]);
      textBufRef.current = "";
      toolCallsRef.current = [];

      const controller = startAgentChat(
        agentId,
        text,
        sessionId,
        // onEvent
        (event) => {
          try {
            const data = JSON.parse(event.data);

            if (event.type === "text") {
              textBufRef.current += data.text || "";
              if (rafRef.current == null) {
                rafRef.current = requestAnimationFrame(flushText);
              }
            } else if (event.type === "tool_use") {
              const part: ToolCallPart = {
                type: "tool-call",
                toolCallId: data.id || `tool-${Date.now()}`,
                toolName: data.name || "unknown",
                args: (data.input || {}) as Record<string, string | number | boolean | null>,
              };
              toolCallsRef.current = [...toolCallsRef.current, part];
              setStreamingToolCalls(toolCallsRef.current);
            } else if (event.type === "tool_result") {
              const prev = toolCallsRef.current;
              if (prev.length > 0) {
                const last = prev[prev.length - 1];
                if (last && last.type === "tool-call") {
                  const updated = [...prev];
                  updated[updated.length - 1] = {
                    ...last,
                    result: data.content ?? data.output ?? "done",
                  };
                  toolCallsRef.current = updated;
                  setStreamingToolCalls(updated);
                }
              }
            } else if (event.type === "result") {
              // Result event carries the complete response text AND cost/turns.
              // Only use result.text if nothing was streamed via text events.
              if (data.text && !textBufRef.current) {
                textBufRef.current = data.text;
                if (rafRef.current == null) {
                  rafRef.current = requestAnimationFrame(flushText);
                }
              }
              setResultMeta({
                cost: data.cost || 0,
                turns: data.turns || 0,
              });
            }
          } catch {
            // Non-JSON event data (e.g., plain text fallback)
            if (event.type === "text") {
              textBufRef.current += event.data;
              if (rafRef.current == null) {
                rafRef.current = requestAnimationFrame(flushText);
              }
            }
          }
        },
        // onDone
        () => {
          // Cancel any pending rAF
          if (rafRef.current != null) {
            cancelAnimationFrame(rafRef.current);
            rafRef.current = null;
          }

          // Read final values from refs (no nested setState)
          const finalText = textBufRef.current;
          const finalTools = toolCallsRef.current;

          const parts: ContentPart[] = [];
          if (finalText) parts.push({ type: "text", text: finalText });
          parts.push(...finalTools);

          if (parts.length > 0) {
            setMessages((prev) => [
              ...prev,
              { role: "assistant" as const, content: parts },
            ]);
          }

          // Clear streaming state
          setStreamingText("");
          setStreamingToolCalls([]);
          textBufRef.current = "";
          toolCallsRef.current = [];
          setIsStreaming(false);
        },
        // onError
        (err) => {
          console.error("Agent chat error:", err);
          setMessages((prev) => [
            ...prev,
            { role: "assistant" as const, content: `Error: ${err}` },
          ]);
          setStreamingText("");
          setStreamingToolCalls([]);
          textBufRef.current = "";
          toolCallsRef.current = [];
          setIsStreaming(false);
        }
      );

      abortRef.current = controller;
    },
    [agentId, sessionId, flushText]
  );

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    stopAgentChat(agentId, sessionId).catch(() => {});
  }, [agentId, sessionId]);

  const threadMessages = useMemo((): ThreadMessageLike[] => {
    if (!isStreaming) return messages;
    const parts: ContentPart[] = [];
    if (streamingText) parts.push({ type: "text", text: streamingText });
    parts.push(...streamingToolCalls);
    return parts.length > 0
      ? [...messages, { role: "assistant" as const, content: parts }]
      : messages;
  }, [messages, isStreaming, streamingText, streamingToolCalls]);

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
  const runtime = useExternalStoreRuntime({
    isRunning: isStreaming,
    messages,
    convertMessage: (msg) => msg,
    onNew: async (message) => {
      await onNew(message);
    },
    onCancel: async () => {
      onCancel();
    },
  });

  return (
    <AssistantRuntimeProvider runtime={runtime}>
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
