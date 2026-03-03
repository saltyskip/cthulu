import type { ThreadMessageLike } from "@assistant-ui/react";
import type { FileOp, PlanOp } from "./FilePreviewContext";
import type { TodoItem } from "../ToolRenderers";

const PLAN_PATH_RE = /\.claude\/plans\/.*\.md$/;

/** Extract all Edit/Write file operations from messages. */
export function extractFileOps(messages: ThreadMessageLike[]): FileOp[] {
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
      } else if (p.toolName === "Write" && args.file_path && !PLAN_PATH_RE.test(args.file_path as string)) {
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

/** Extract plan files (Write calls to .claude/plans/*.md) from messages. */
export function extractPlans(messages: ThreadMessageLike[]): PlanOp[] {
  const ops: PlanOp[] = [];
  for (const msg of messages) {
    const content = msg.content;
    if (!Array.isArray(content)) continue;
    for (const part of content) {
      const p = part as Record<string, unknown>;
      if (p.type !== "tool-call") continue;
      const args = p.args as Record<string, unknown> | undefined;
      if (!args) continue;
      if (p.toolName === "Write" && args.file_path && PLAN_PATH_RE.test(args.file_path as string)) {
        ops.push({
          toolCallId: (p.toolCallId as string) || "",
          filePath: args.file_path as string,
          content: args.content as string | undefined,
        });
      }
    }
  }
  return ops;
}

/** Get the filename from a path. */
export function basename(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  return parts.pop() || filePath;
}

/** Scan messages backwards for the most recent TodoWrite tool call and extract its todos. */
export function extractLatestTodos(messages: ThreadMessageLike[]): TodoItem[] | null {
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

/** A step in the stepper: one user message and the file ops it triggered. */
export interface TaskStep {
  userMessage: string;
  messageIndex: number;
  fileOps: FileOp[];
}

/**
 * Group file operations by the user message that triggered them.
 * Walks messages in order, tracking the latest user message.
 * Each assistant message's file ops are attributed to the preceding user message.
 * Steps with zero file ops are skipped.
 */
export function groupFileOpsByTask(messages: ThreadMessageLike[]): TaskStep[] {
  const steps: TaskStep[] = [];
  let currentUserText = "";
  let currentUserIdx = -1;
  let currentOps: FileOp[] = [];

  const flushStep = () => {
    if (currentOps.length > 0 && currentUserIdx >= 0) {
      // Dedup by filePath within the step (keep latest)
      const deduped = new Map<string, FileOp>();
      for (const op of currentOps) deduped.set(op.filePath, op);
      steps.push({
        userMessage: currentUserText,
        messageIndex: currentUserIdx,
        fileOps: [...deduped.values()],
      });
    }
    currentOps = [];
  };

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.role === "user") {
      flushStep();
      // Extract text from user message
      const content = msg.content;
      if (typeof content === "string") {
        currentUserText = content;
      } else if (Array.isArray(content)) {
        currentUserText = (content as Array<Record<string, unknown>>)
          .filter((p) => p.type === "text" && p.text)
          .map((p) => p.text as string)
          .join(" ");
      } else {
        currentUserText = "";
      }
      currentUserIdx = i;
    } else if (msg.role === "assistant") {
      // Extract file ops from this assistant message
      const content = msg.content;
      if (!Array.isArray(content)) continue;
      for (const part of content) {
        const p = part as Record<string, unknown>;
        if (p.type !== "tool-call") continue;
        const args = p.args as Record<string, unknown> | undefined;
        if (!args) continue;
        const toolCallId = (p.toolCallId as string) || "";
        if (p.toolName === "Edit" && args.file_path) {
          currentOps.push({
            toolCallId,
            filePath: args.file_path as string,
            type: "edit",
            oldString: args.old_string as string | undefined,
            newString: args.new_string as string | undefined,
          });
        } else if (p.toolName === "Write" && args.file_path && !PLAN_PATH_RE.test(args.file_path as string)) {
          currentOps.push({
            toolCallId,
            filePath: args.file_path as string,
            type: "write",
            content: args.content as string | undefined,
          });
        }
      }
    }
  }
  flushStep(); // flush last step
  return steps;
}

/** Convert a File to base64 string (without the data URL prefix). */
export function fileToBase64(file: File): Promise<string> {
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
