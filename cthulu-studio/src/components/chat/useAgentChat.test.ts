import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useAgentChat } from "./useAgentChat";
import type { InteractSSEEvent } from "../../api/interactStream";

// Mock API modules
vi.mock("../../api/client", () => ({
  getServerUrl: () => "http://localhost:8081",
  getSessionLog: vi.fn().mockResolvedValue([]),
  getSessionStatus: vi.fn().mockResolvedValue({ busy: false, process_alive: false }),
  stopAgentChat: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../api/interactStream", () => ({
  startAgentChat: vi.fn(),
  reconnectAgentChat: vi.fn(),
}));

// Import mocked modules for control
import { getSessionLog, getSessionStatus } from "../../api/client";
import { startAgentChat, reconnectAgentChat } from "../../api/interactStream";

const mockGetSessionLog = vi.mocked(getSessionLog);
const mockGetSessionStatus = vi.mocked(getSessionStatus);
const mockStartAgentChat = vi.mocked(startAgentChat);
const mockReconnectAgentChat = vi.mocked(reconnectAgentChat);

// Helper to capture the onEvent/onDone/onError callbacks from startAgentChat
function captureStartCallbacks() {
  let onEvent: (event: InteractSSEEvent) => void = () => {};
  let onDone: () => void = () => {};
  let onError: (err: string) => void = () => {};
  const controller = new AbortController();

  mockStartAgentChat.mockImplementation(
    (_agentId, _prompt, _sessionId, ev, done, err) => {
      onEvent = ev;
      onDone = done;
      onError = err;
      return controller;
    },
  );

  return {
    getOnEvent: () => onEvent,
    getOnDone: () => onDone,
    getOnError: () => onError,
    controller,
  };
}

function captureReconnectCallbacks() {
  let onEvent: (event: InteractSSEEvent) => void = () => {};
  let onDone: () => void = () => {};
  let onError: (err: string) => void = () => {};
  const controller = new AbortController();

  mockReconnectAgentChat.mockImplementation(
    (_agentId, _sessionId, ev, done, err) => {
      onEvent = ev;
      onDone = done;
      onError = err;
      return controller;
    },
  );

  return {
    getOnEvent: () => onEvent,
    getOnDone: () => onDone,
    getOnError: () => onError,
    controller,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockGetSessionLog.mockResolvedValue([]);
  mockGetSessionStatus.mockResolvedValue({ busy: false, process_alive: false } as never);
  // Provide a default no-op for startAgentChat
  mockStartAgentChat.mockImplementation(() => new AbortController());
});

describe("useAgentChat", () => {
  it("initializes with empty messages and not streaming", () => {
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));
    expect(result.current.messages).toEqual([]);
    expect(result.current.isStreaming).toBe(false);
    expect(result.current.resultMeta).toBeNull();
    expect(result.current.attachments).toEqual([]);
  });

  it("restores history from session log on mount", async () => {
    mockGetSessionLog.mockResolvedValue([
      'user:{"text":"hello"}',
      'text:{"text":"hi there"}',
      "done:{}",
    ]);

    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    // Wait for async log fetch
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    expect(result.current.messages).toHaveLength(2);
  });

  it("sends a message and enters streaming state", async () => {
    const { getOnDone } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "test message" });
    });

    expect(mockStartAgentChat).toHaveBeenCalledWith(
      "agent-1",
      "test message",
      "session-1",
      expect.any(Function),
      expect.any(Function),
      expect.any(Function),
      undefined,
      undefined,
    );
    expect(result.current.isStreaming).toBe(true);

    // Complete the stream
    act(() => { getOnDone()(); });
    expect(result.current.isStreaming).toBe(false);
  });

  it("ignores empty messages", async () => {
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "   " });
    });

    expect(mockStartAgentChat).not.toHaveBeenCalled();
  });

  it("processes text SSE events", async () => {
    const { getOnEvent, getOnDone } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "hello" });
    });

    // Simulate text event
    act(() => {
      getOnEvent()({ type: "text", data: JSON.stringify({ text: "Hello!" }) });
    });

    // Flush rAF manually — happy-dom may not auto-fire
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    // Complete
    act(() => { getOnDone()(); });

    // Should have user + assistant messages
    expect(result.current.messages).toHaveLength(2);
    expect(result.current.isStreaming).toBe(false);
  });

  it("processes tool_use and tool_result events", async () => {
    const { getOnEvent, getOnDone } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "read file" });
    });

    act(() => {
      getOnEvent()({
        type: "tool_use",
        data: JSON.stringify({ tool: "Read", input: { file_path: "/tmp/a.ts" }, id: "tc-1" }),
      });
    });

    act(() => {
      getOnEvent()({
        type: "tool_result",
        data: JSON.stringify({ content: "file contents" }),
      });
    });

    act(() => { getOnDone()(); });

    expect(result.current.messages).toHaveLength(2);
    const assistant = result.current.messages[1];
    const content = assistant.content as { type: string; result?: unknown }[];
    expect(content.some((p) => p.type === "tool-call")).toBe(true);
  });

  it("handles result event with cost metadata", async () => {
    const { getOnEvent, getOnDone } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "hi" });
    });

    act(() => {
      getOnEvent()({
        type: "result",
        data: JSON.stringify({ text: "done", cost: 0.05, turns: 3 }),
      });
    });

    act(() => { getOnDone()(); });

    expect(result.current.resultMeta).toEqual({ cost: 0.05, turns: 3 });
  });

  it("handles error during streaming", async () => {
    const { getOnError } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "hello" });
    });

    act(() => { getOnError()("Network error"); });

    expect(result.current.isStreaming).toBe(false);
    // Error message added
    expect(result.current.messages).toHaveLength(2); // user + error
    const errMsg = result.current.messages[1];
    expect(errMsg.role).toBe("assistant");
    expect(String(errMsg.content)).toContain("Error:");
  });

  it("maps 409 error to busy message", async () => {
    const { getOnError } = captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({ content: "hello" });
    });

    act(() => { getOnError()("HTTP 409 conflict"); });

    const errMsg = result.current.messages[1];
    expect(String(errMsg.content)).toContain("busy");
  });

  it("handleCancel aborts and calls stopAgentChat", async () => {
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    act(() => { result.current.handleCancel(); });

    const { stopAgentChat } = await import("../../api/client");
    expect(stopAgentChat).toHaveBeenCalledWith("agent-1", "session-1");
  });

  it("reconnects on mount when session is busy", async () => {
    mockGetSessionStatus.mockResolvedValue({ busy: true, process_alive: true } as never);
    const { getOnEvent, getOnDone } = captureReconnectCallbacks();

    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(mockReconnectAgentChat).toHaveBeenCalled();
    expect(result.current.isStreaming).toBe(true);

    // Simulate some text and done
    act(() => {
      getOnEvent()({ type: "text", data: JSON.stringify({ text: "reconnected" }) });
    });

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    act(() => { getOnDone()(); });

    expect(result.current.isStreaming).toBe(false);
    expect(result.current.messages.length).toBeGreaterThan(0);
  });

  it("extracts text from array content", async () => {
    captureStartCallbacks();
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    await act(async () => {
      await result.current.handleSend({
        content: [
          { type: "text", text: "part 1" },
          { type: "text", text: "part 2" },
        ],
      });
    });

    expect(mockStartAgentChat).toHaveBeenCalledWith(
      "agent-1",
      "part 1\npart 2",
      expect.anything(),
      expect.any(Function),
      expect.any(Function),
      expect.any(Function),
      undefined,
      undefined,
    );
  });

  it("manages file attachments", () => {
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    // Add files
    const mockFile = new File(["data"], "test.png", { type: "image/png" });
    act(() => {
      result.current.addFiles([mockFile]);
    });

    expect(result.current.attachments).toHaveLength(1);
    expect(result.current.attachments[0].file).toBe(mockFile);

    // Remove
    const id = result.current.attachments[0].id;
    act(() => {
      result.current.removeAttachment(id);
    });

    expect(result.current.attachments).toHaveLength(0);
  });

  it("filters non-image files in addFiles", () => {
    const { result } = renderHook(() => useAgentChat("agent-1", "session-1"));

    const textFile = new File(["data"], "readme.txt", { type: "text/plain" });
    act(() => {
      result.current.addFiles([textFile]);
    });

    expect(result.current.attachments).toHaveLength(0);
  });
});
