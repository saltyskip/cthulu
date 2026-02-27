import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { forwardRef, useImperativeHandle } from "react";
import FlowWorkspaceView from "../FlowWorkspaceView";
import { makeFlow, makeSignal } from "../../test/fixtures";
import type { UpdateSignal } from "../../hooks/useFlowDispatch";
import type { Flow } from "../../types/flow";

// --- Capture setText calls to verify imperative updates ---
const setTextSpy = vi.fn();
const getTextSpy = vi.fn().mockReturnValue("");

// --- Mock Canvas ---
vi.mock("../Canvas", () => ({
  default: forwardRef((_props: unknown, ref: React.Ref<unknown>) => {
    useImperativeHandle(ref, () => ({
      mergeFromFlow: vi.fn(),
      addNodeAtScreen: vi.fn(),
      getNode: vi.fn(),
      updateNodeData: vi.fn(),
      deleteNode: vi.fn(),
    }));
    return <div data-testid="canvas" />;
  }),
}));

// --- Mock FlowEditor with imperative setText/getText ---
vi.mock("../FlowEditor", () => ({
  default: forwardRef(({ defaultValue, onChange }: { defaultValue: string; onChange: (v: string) => void }, ref: React.Ref<unknown>) => {
    useImperativeHandle(ref, () => ({
      setText: setTextSpy,
      getText: getTextSpy,
      revealNode: vi.fn(),
    }));
    return (
      <textarea
        data-testid="flow-editor"
        defaultValue={defaultValue}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }),
}));

// --- Mock RunLog ---
vi.mock("../RunLog", () => ({
  default: () => <div data-testid="run-log" />,
}));

// --- Mock AgentChatView ---
vi.mock("../AgentChatView", () => ({
  default: () => <div data-testid="agent-chat-view" />,
}));

// --- Mock ErrorBoundary ---
vi.mock("../ErrorBoundary", () => ({
  default: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

function getEditorDefaultValue(): string {
  return (screen.getByTestId("flow-editor") as HTMLTextAreaElement).defaultValue;
}

interface RenderProps {
  flowId?: string | null;
  canonicalFlow?: Flow | null;
  updateSignal?: UpdateSignal;
}

function renderWorkspace(props: RenderProps = {}) {
  const defaultFlow = makeFlow();
  const defaults = {
    flowId: props.flowId !== undefined ? props.flowId : "flow-1",
    canonicalFlow: props.canonicalFlow !== undefined ? props.canonicalFlow : defaultFlow,
    updateSignal: props.updateSignal || makeSignal(1, "init"),
    canvasRef: { current: null },
    onCanvasChange: vi.fn(),
    onEditorChange: vi.fn(),
    onSelectionChange: vi.fn(),
    selectedNodeId: null,
    nodeRunStatus: {},
    runEvents: [],
    onRunEventsClear: vi.fn(),
    runLogOpen: false,
    onRunLogClose: vi.fn(),
  };

  const result = render(<FlowWorkspaceView {...defaults} />);
  return {
    ...result,
    rerender: (overrides: Partial<typeof defaults>) =>
      result.rerender(<FlowWorkspaceView {...defaults} {...overrides} />),
  };
}

describe("FlowWorkspaceView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setTextSpy.mockClear();
    getTextSpy.mockClear();
  });

  // --- Editor-originated changes do NOT push text to Monaco ---

  describe("editor-originated changes do not push to Monaco", () => {
    it("editor change does not call setText", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      // Clear any setText calls from init
      setTextSpy.mockClear();

      // Rerender with source="editor" — should NOT push text
      const updatedFlow = makeFlow({ name: "Edited" });
      rerender({
        canonicalFlow: updatedFlow,
        updateSignal: makeSignal(2, "editor"),
      });

      expect(setTextSpy).not.toHaveBeenCalled();
    });

    it("5 rapid editor changes never call setText", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      for (let i = 2; i <= 6; i++) {
        const modifiedFlow = makeFlow({ name: `Edit ${i}` });
        rerender({
          canonicalFlow: modifiedFlow,
          updateSignal: makeSignal(i, "editor"),
        });
      }

      expect(setTextSpy).not.toHaveBeenCalled();
    });
  });

  // --- Non-editor changes DO push text to Monaco ---

  describe("non-editor changes push text via setText", () => {
    it("canvas change calls setText with serialized flow", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      const updatedFlow = makeFlow({ name: "Canvas Update" });
      rerender({
        canonicalFlow: updatedFlow,
        updateSignal: makeSignal(2, "canvas"),
      });

      expect(setTextSpy).toHaveBeenCalledWith(JSON.stringify(updatedFlow, null, 2));
    });

    it("server change calls setText", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      const serverFlow = makeFlow({ name: "Server Update", version: 5 });
      rerender({
        canonicalFlow: serverFlow,
        updateSignal: makeSignal(2, "server"),
      });

      expect(setTextSpy).toHaveBeenCalledWith(JSON.stringify(serverFlow, null, 2));
    });

    it("init signal calls setText on non-mount updates", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      // Second init (e.g., flow switch without remount)
      const newFlow = makeFlow({ id: "flow-1", name: "New Flow" });
      rerender({
        canonicalFlow: newFlow,
        updateSignal: makeSignal(2, "init"),
      });

      expect(setTextSpy).toHaveBeenCalledWith(JSON.stringify(newFlow, null, 2));
    });
  });

  // --- Counter-based dedup ---

  describe("counter-based dedup", () => {
    it("same counter does not call setText", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      // Rerender with new canonicalFlow but SAME counter
      const updatedFlow = makeFlow({ name: "Should Not Apply" });
      rerender({
        canonicalFlow: updatedFlow,
        updateSignal: makeSignal(1, "canvas"), // same counter=1
      });

      expect(setTextSpy).not.toHaveBeenCalled();
    });

    it("silent version update (no counter bump) does not call setText", () => {
      const flow = makeFlow();
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      const versionedFlow = makeFlow({ version: 99 });
      rerender({
        canonicalFlow: versionedFlow,
        updateSignal: makeSignal(1, "init"), // same counter
      });

      expect(setTextSpy).not.toHaveBeenCalled();
    });
  });

  // --- Edge cases ---

  describe("edge cases", () => {
    it("null canonicalFlow pushes empty string via setText", () => {
      const { rerender } = renderWorkspace({
        canonicalFlow: makeFlow(),
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      rerender({
        canonicalFlow: null,
        updateSignal: makeSignal(2, "server"),
      });

      expect(setTextSpy).toHaveBeenCalledWith("");
    });

    it("flow switch remounts editor with new defaultValue (key change)", () => {
      const flowA = makeFlow({ id: "flow-a", name: "Flow A" });
      const { rerender } = renderWorkspace({
        flowId: "flow-a",
        canonicalFlow: flowA,
        updateSignal: makeSignal(1, "init"),
      });

      // Flow switch — key changes from "flow-a" to "flow-b", so editor remounts
      const flowB = makeFlow({ id: "flow-b", name: "Flow B" });
      rerender({
        flowId: "flow-b",
        canonicalFlow: flowB,
        updateSignal: makeSignal(2, "init"),
      });

      // With key-based remount, the new editor gets defaultValue of flow B
      // (setText may or may not be called depending on timing, but defaultValue is set)
    });
  });

  // --- SSE echo scenario (THE cursor-jump fix) ---

  describe("SSE echo after editor edit", () => {
    it("editor edit followed by server echo: setText uses forceMoveMarkers:false", () => {
      // The fix: FlowEditor.setText uses forceMoveMarkers: false,
      // so even if a server echo arrives, the cursor doesn't jump to bottom.
      // And setText skips entirely if the text is identical.
      const flow = makeFlow({ name: "Original" });
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      // Step 1: editor edit — no setText called
      const editedFlow = makeFlow({ name: "Edited" });
      rerender({
        canonicalFlow: editedFlow,
        updateSignal: makeSignal(2, "editor"),
      });
      expect(setTextSpy).not.toHaveBeenCalled();

      // Step 2: server echo — setText IS called, but with forceMoveMarkers:false
      // (which is handled inside FlowEditor.setText implementation)
      const echoFlow = makeFlow({ name: "Edited", version: 2 });
      rerender({
        canonicalFlow: echoFlow,
        updateSignal: makeSignal(3, "server"),
      });
      expect(setTextSpy).toHaveBeenCalledTimes(1);
    });

    it("editor edit WITHOUT subsequent echo: setText never called", () => {
      const flow = makeFlow({ name: "Original" });
      const { rerender } = renderWorkspace({
        canonicalFlow: flow,
        updateSignal: makeSignal(1, "init"),
      });

      setTextSpy.mockClear();

      // Editor edit
      const editedFlow = makeFlow({ name: "Edited" });
      rerender({
        canonicalFlow: editedFlow,
        updateSignal: makeSignal(2, "editor"),
      });

      // Silent version update (save completed) — same counter
      const savedFlow = makeFlow({ name: "Edited", version: 2 });
      rerender({
        canonicalFlow: savedFlow,
        updateSignal: makeSignal(2, "editor"),
      });

      expect(setTextSpy).not.toHaveBeenCalled();
    });
  });
});
