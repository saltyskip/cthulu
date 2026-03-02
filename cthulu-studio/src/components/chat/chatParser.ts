import type { ThreadMessageLike } from "@assistant-ui/react";

// Internal mutable type for streaming content parts.
export interface TextPart { type: "text"; text: string }
export interface ToolCallPart {
  type: "tool-call";
  toolCallId: string;
  toolName: string;
  args: Record<string, string | number | boolean | null>;
  result?: unknown;
}
export type ContentPart = TextPart | ToolCallPart;

/** Replay JSONL log lines into ThreadMessageLike[] messages. */
export function replayLogLines(lines: string[]): ThreadMessageLike[] {
  const messages: ThreadMessageLike[] = [];
  let currentParts: ContentPart[] = [];

  const flushAssistant = () => {
    if (currentParts.length > 0) {
      // Mark any unresolved tool calls as completed (tool_result may not be in the log)
      for (const part of currentParts) {
        if (part.type === "tool-call" && !(part as ToolCallPart).result) {
          (part as ToolCallPart).result = "done";
        }
      }
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
        // Always add result text — it's the final assistant response for this turn
        if (data.text) {
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
