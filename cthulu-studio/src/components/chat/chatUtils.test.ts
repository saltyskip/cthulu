import { describe, it, expect } from "vitest";
import { extractFileOps, basename, extractLatestTodos } from "./chatUtils";
import type { ThreadMessageLike } from "@assistant-ui/react";

function makeToolCallMessage(
  toolName: string,
  args: Record<string, unknown>,
  toolCallId = "tc-1",
): ThreadMessageLike {
  return {
    role: "assistant",
    content: [
      {
        type: "tool-call" as const,
        toolCallId,
        toolName,
        args,
        result: "done",
      },
    ],
  };
}

describe("extractFileOps", () => {
  it("returns empty for no messages", () => {
    expect(extractFileOps([])).toEqual([]);
  });

  it("returns empty for messages with no tool calls", () => {
    const msgs: ThreadMessageLike[] = [
      { role: "assistant", content: "just text" },
    ];
    expect(extractFileOps(msgs)).toEqual([]);
  });

  it("extracts Edit operations", () => {
    const msgs = [
      makeToolCallMessage("Edit", {
        file_path: "/src/app.ts",
        old_string: "foo",
        new_string: "bar",
      }),
    ];
    const ops = extractFileOps(msgs);
    expect(ops).toHaveLength(1);
    expect(ops[0]).toMatchObject({
      filePath: "/src/app.ts",
      type: "edit",
      oldString: "foo",
      newString: "bar",
    });
  });

  it("extracts Write operations", () => {
    const msgs = [
      makeToolCallMessage("Write", {
        file_path: "/src/new.ts",
        content: "export default {}",
      }),
    ];
    const ops = extractFileOps(msgs);
    expect(ops).toHaveLength(1);
    expect(ops[0]).toMatchObject({
      filePath: "/src/new.ts",
      type: "write",
      content: "export default {}",
    });
  });

  it("ignores non-Edit/Write tool calls", () => {
    const msgs = [
      makeToolCallMessage("Read", { file_path: "/src/app.ts" }),
      makeToolCallMessage("Bash", { command: "ls" }),
    ];
    expect(extractFileOps(msgs)).toEqual([]);
  });

  it("extracts multiple ops across messages", () => {
    const msgs = [
      makeToolCallMessage("Edit", { file_path: "/a.ts", old_string: "x", new_string: "y" }, "tc-1"),
      makeToolCallMessage("Write", { file_path: "/b.ts", content: "z" }, "tc-2"),
    ];
    const ops = extractFileOps(msgs);
    expect(ops).toHaveLength(2);
    expect(ops[0].filePath).toBe("/a.ts");
    expect(ops[1].filePath).toBe("/b.ts");
  });

  it("skips tool calls without file_path", () => {
    const msgs = [makeToolCallMessage("Edit", { old_string: "a", new_string: "b" })];
    expect(extractFileOps(msgs)).toEqual([]);
  });
});

describe("basename", () => {
  it("extracts filename from unix path", () => {
    expect(basename("/src/components/App.tsx")).toBe("App.tsx");
  });

  it("extracts filename from windows path", () => {
    expect(basename("C:\\Users\\foo\\bar.ts")).toBe("bar.ts");
  });

  it("returns input if no separators", () => {
    expect(basename("file.txt")).toBe("file.txt");
  });

  it("handles trailing slash by returning empty string fallback", () => {
    // pop() returns "" for trailing slash, which is falsy, so falls back to full path
    expect(basename("/src/components/")).toBe("/src/components/");
  });
});

describe("extractLatestTodos", () => {
  it("returns null for no messages", () => {
    expect(extractLatestTodos([])).toBeNull();
  });

  it("returns null when no TodoWrite calls exist", () => {
    const msgs: ThreadMessageLike[] = [
      { role: "assistant", content: "hello" },
    ];
    expect(extractLatestTodos(msgs)).toBeNull();
  });

  it("extracts todos from the latest TodoWrite call", () => {
    const todos = [
      { content: "Task 1", status: "completed", activeForm: "Doing 1" },
      { content: "Task 2", status: "in_progress", activeForm: "Doing 2" },
    ];
    const msgs = [
      makeToolCallMessage("TodoWrite", { todos }, "tc-1"),
    ];
    const result = extractLatestTodos(msgs);
    expect(result).toEqual(todos);
  });

  it("returns the most recent TodoWrite (last message wins)", () => {
    const old = [{ content: "Old task", status: "pending", activeForm: "Old" }];
    const latest = [{ content: "New task", status: "in_progress", activeForm: "New" }];
    const msgs = [
      makeToolCallMessage("TodoWrite", { todos: old }, "tc-1"),
      makeToolCallMessage("TodoWrite", { todos: latest }, "tc-2"),
    ];
    const result = extractLatestTodos(msgs);
    expect(result).toEqual(latest);
  });

  it("returns null if TodoWrite args have no todos array", () => {
    const msgs = [makeToolCallMessage("TodoWrite", { something: "else" })];
    expect(extractLatestTodos(msgs)).toBeNull();
  });
});
