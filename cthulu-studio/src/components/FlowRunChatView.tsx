import { useState, useEffect, useCallback, useMemo } from "react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  ThreadPrimitive,
  type ThreadMessageLike,
} from "@assistant-ui/react";
import {
  CompactAssistantMessage,
  CompactUserMessage,
} from "./ChatPrimitives";
import { getSessionLog, streamSessionLog } from "../api/client";
import type { FlowRunMeta } from "../api/client";

interface FlowRunChatViewProps {
  agentId: string;
  sessionId: string;
  busy: boolean;
  flowRun?: FlowRunMeta;
}

// ── Stream-JSON → assistant-ui message conversion ───────────────────

interface RawLine {
  raw: string;
  parsed: Record<string, unknown> | null;
}

function parseRawLine(raw: string): RawLine {
  try {
    return { raw, parsed: JSON.parse(raw) };
  } catch {
    return { raw, parsed: null };
  }
}

function linesToMessages(lines: RawLine[]): ThreadMessageLike[] {
  const messages: ThreadMessageLike[] = [];

  type ContentPart = ThreadMessageLike["content"] extends string | infer U
    ? U extends readonly (infer P)[]
      ? P
      : never
    : never;

  let currentParts: ContentPart[] = [];

  const flushAssistant = () => {
    if (currentParts.length > 0) {
      messages.push({
        role: "assistant" as const,
        content: currentParts,
      });
      currentParts = [];
    }
  };

  for (const { parsed } of lines) {
    if (!parsed) continue;
    const eventType = parsed.type as string;

    if (eventType === "system") continue;

    if (eventType === "assistant") {
      const content = (parsed.message as Record<string, unknown>)
        ?.content as Array<Record<string, unknown>> | undefined;
      if (!Array.isArray(content)) continue;

      for (const block of content) {
        const blockType = block.type as string;
        if (blockType === "text" && block.text) {
          currentParts.push({
            type: "text" as const,
            text: block.text as string,
          });
        } else if (blockType === "tool_use") {
          const toolCallId =
            (block.id as string) ||
            `tool-${messages.length}-${currentParts.length}`;
          currentParts.push({
            type: "tool-call" as const,
            toolCallId,
            toolName: (block.name as string) || "unknown",
            args: (typeof block.input === "object" && block.input !== null
              ? block.input
              : {}) as Record<string, string | number | boolean | null>,
            result:
              block.result !== undefined
                ? (block.result as unknown)
                : undefined,
          });
        } else if (blockType === "tool_result") {
          const resultContent =
            typeof block.content === "string"
              ? block.content
              : JSON.stringify(block.content);
          currentParts.push({
            type: "text" as const,
            text: `\`\`\`\n${resultContent}\n\`\`\``,
          });
        }
      }
      flushAssistant();
      continue;
    }

    if (eventType === "result") {
      flushAssistant();
      const resultText = (parsed.result as string) || "";
      if (resultText) {
        messages.push({
          role: "assistant" as const,
          content: resultText,
        });
      }
      continue;
    }

    if (
      eventType === "content_block_start" &&
      (parsed.content_block as Record<string, unknown>)?.type === "tool_use"
    ) {
      const cb = parsed.content_block as Record<string, unknown>;
      const toolCallId =
        (cb.id as string) ||
        `tool-${messages.length}-${currentParts.length}`;
      currentParts.push({
        type: "tool-call" as const,
        toolCallId,
        toolName: (cb.name as string) || "unknown",
        args: {} as Record<string, string | number | boolean | null>,
      });
      continue;
    }
  }

  flushAssistant();
  return messages;
}

// ── Main component ──────────────────────────────────────────────────

function FlowRunThread({
  messages,
  isRunning,
  flowRun,
  resultMeta,
}: {
  messages: ThreadMessageLike[];
  isRunning: boolean;
  flowRun?: FlowRunMeta;
  resultMeta: { cost: number; turns: number } | null;
}) {
  const runtime = useExternalStoreRuntime({
    isRunning,
    messages,
    convertMessage: (msg) => msg,
    onNew: async () => {},
  });

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <div className="fr-wrap">
        {flowRun && (
          <div className="fr-bar">
            <span className="fr-bar-label">
              {flowRun.flow_name} &mdash; {flowRun.node_label}
            </span>
            <span className="fr-bar-meta">
              {isRunning ? (
                <span className="fr-bar-live">● live</span>
              ) : resultMeta ? (
                <>{resultMeta.turns}t · ${resultMeta.cost.toFixed(4)}</>
              ) : null}
              <span className="fr-bar-id">{flowRun.run_id.slice(0, 8)}</span>
            </span>
          </div>
        )}

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
      </div>
    </AssistantRuntimeProvider>
  );
}

export default function FlowRunChatView({
  agentId,
  sessionId,
  busy,
  flowRun,
}: FlowRunChatViewProps) {
  const [rawLines, setRawLines] = useState<RawLine[]>([]);
  const [isLive, setIsLive] = useState(busy);

  const addLine = useCallback((raw: string) => {
    const parsed = parseRawLine(raw);
    setRawLines((prev) => [...prev, parsed]);
  }, []);

  useEffect(() => {
    setRawLines([]);
    setIsLive(busy);

    if (busy) {
      const cleanup = streamSessionLog(agentId, sessionId, addLine, () =>
        setIsLive(false)
      );
      return cleanup;
    } else {
      (async () => {
        try {
          const lines = await getSessionLog(agentId, sessionId);
          setRawLines(lines.filter((l) => l.trim()).map(parseRawLine));
        } catch {
          // ignore
        }
      })();
    }
  }, [agentId, sessionId, busy, addLine]);

  const messages = useMemo(() => linesToMessages(rawLines), [rawLines]);

  const resultMeta = useMemo(() => {
    for (let i = rawLines.length - 1; i >= 0; i--) {
      const p = rawLines[i].parsed;
      if (p && (p.type as string) === "result") {
        return {
          cost: (p.total_cost_usd as number) || 0,
          turns: (p.num_turns as number) || 0,
        };
      }
    }
    return null;
  }, [rawLines]);

  return (
    <FlowRunThread
      messages={messages}
      isRunning={isLive}
      flowRun={flowRun}
      resultMeta={resultMeta}
    />
  );
}
