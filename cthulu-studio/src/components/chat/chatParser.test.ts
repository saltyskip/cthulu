import { describe, it, expect } from "vitest";
import { replayLogLines } from "./chatParser";

// Helpers to build JSONL log lines
const userLine = (text: string) => `user:${JSON.stringify({ text })}`;
const textLine = (text: string) => `text:${JSON.stringify({ text })}`;
const toolUseLine = (tool: string, input: Record<string, unknown>, id?: string) =>
  `tool_use:${JSON.stringify({ tool, input, id: id || `tool-${Date.now()}` })}`;
const toolResultLine = (content: string) =>
  `tool_result:${JSON.stringify({ content })}`;
const resultLine = (text: string, cost = 0.01, turns = 1) =>
  `result:${JSON.stringify({ text, cost, turns })}`;
const doneLine = () => "done:{}";

describe("replayLogLines", () => {
  it("returns empty array for empty input", () => {
    expect(replayLogLines([])).toEqual([]);
  });

  it("returns empty array for lines without colons", () => {
    expect(replayLogLines(["no-colon-here", "another"])).toEqual([]);
  });

  it("parses a single user message", () => {
    const messages = replayLogLines([userLine("hello")]);
    expect(messages).toHaveLength(1);
    expect(messages[0].role).toBe("user");
    const content = messages[0].content as { type: string; text: string }[];
    expect(content[0].text).toBe("hello");
  });

  it("parses a simple user + assistant text exchange", () => {
    const messages = replayLogLines([
      userLine("hi"),
      textLine("Hello! "),
      textLine("How can I help?"),
      doneLine(),
    ]);
    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe("user");
    expect(messages[1].role).toBe("assistant");
    // Text parts should be merged
    const content = messages[1].content as { type: string; text: string }[];
    expect(content).toHaveLength(1);
    expect(content[0].text).toBe("Hello! How can I help?");
  });

  it("merges consecutive text events into one part", () => {
    const messages = replayLogLines([
      textLine("a"),
      textLine("b"),
      textLine("c"),
      doneLine(),
    ]);
    expect(messages).toHaveLength(1);
    const content = messages[0].content as { type: string; text: string }[];
    expect(content).toHaveLength(1);
    expect(content[0].text).toBe("abc");
  });

  it("parses tool_use events", () => {
    const messages = replayLogLines([
      toolUseLine("Read", { file_path: "/tmp/foo.ts" }, "tc-1"),
      doneLine(),
    ]);
    expect(messages).toHaveLength(1);
    const content = messages[0].content as { type: string; toolName?: string; toolCallId?: string }[];
    expect(content[0].type).toBe("tool-call");
    expect(content[0].toolName).toBe("Read");
    expect(content[0].toolCallId).toBe("tc-1");
  });

  it("parses tool_use with string input (JSON-encoded args)", () => {
    const messages = replayLogLines([
      `tool_use:${JSON.stringify({ tool: "Bash", input: '{"command":"ls"}', id: "tc-2" })}`,
      doneLine(),
    ]);
    const content = messages[0].content as { type: string; args?: Record<string, unknown> }[];
    expect(content[0].args).toEqual({ command: "ls" });
  });

  it("matches tool_result to the last unresolved tool-call", () => {
    const messages = replayLogLines([
      toolUseLine("Read", { file_path: "/a" }, "tc-1"),
      toolResultLine("file contents here"),
      doneLine(),
    ]);
    const content = messages[0].content as { type: string; result?: unknown }[];
    expect(content[0].result).toBe("file contents here");
  });

  it("marks unresolved tool calls as done on flush", () => {
    const messages = replayLogLines([
      toolUseLine("Bash", { command: "echo hi" }, "tc-1"),
      doneLine(),
    ]);
    const content = messages[0].content as { type: string; result?: unknown }[];
    expect(content[0].result).toBe("done");
  });

  it("handles result event with text", () => {
    const messages = replayLogLines([
      textLine("streamed text"),
      resultLine("result text"),
      doneLine(),
    ]);
    const content = messages[0].content as { type: string; text: string }[];
    // Should have both the streamed text and result text
    expect(content).toHaveLength(2);
    expect(content[0].text).toBe("streamed text");
    expect(content[1].text).toBe("result text");
  });

  it("handles multiple turns (user -> assistant -> user -> assistant)", () => {
    const messages = replayLogLines([
      userLine("first"),
      textLine("response 1"),
      doneLine(),
      userLine("second"),
      textLine("response 2"),
      doneLine(),
    ]);
    expect(messages).toHaveLength(4);
    expect(messages.map((m) => m.role)).toEqual(["user", "assistant", "user", "assistant"]);
  });

  it("flushes remaining parts at end without done event", () => {
    const messages = replayLogLines([
      textLine("incomplete"),
    ]);
    expect(messages).toHaveLength(1);
    expect(messages[0].role).toBe("assistant");
  });

  it("skips corrupt JSON gracefully", () => {
    const messages = replayLogLines([
      "text:{invalid json",
      userLine("valid"),
    ]);
    // Corrupt text is skipped, valid user message is parsed
    expect(messages).toHaveLength(1);
    expect(messages[0].role).toBe("user");
  });

  it("ignores stderr events", () => {
    const messages = replayLogLines([
      "stderr:{\"text\":\"some warning\"}",
      textLine("hello"),
      doneLine(),
    ]);
    expect(messages).toHaveLength(1);
    const content = messages[0].content as { type: string; text: string }[];
    expect(content[0].text).toBe("hello");
  });

  it("handles text + tool interleaved", () => {
    const messages = replayLogLines([
      textLine("Let me read that file."),
      toolUseLine("Read", { file_path: "/foo" }, "tc-1"),
      toolResultLine("contents"),
      textLine("Here's what I found."),
      doneLine(),
    ]);
    expect(messages).toHaveLength(1);
    const content = messages[0].content as { type: string }[];
    expect(content).toHaveLength(3);
    expect(content[0].type).toBe("text");
    expect(content[1].type).toBe("tool-call");
    expect(content[2].type).toBe("text");
  });

  it("generates fallback toolCallId when missing", () => {
    const messages = replayLogLines([
      `tool_use:${JSON.stringify({ tool: "Read", input: {} })}`,
      doneLine(),
    ]);
    const content = messages[0].content as { type: string; toolCallId?: string }[];
    expect(content[0].toolCallId).toMatch(/^tool-replay-/);
  });
});
