import { useMemo, useCallback, useRef } from "react";
import type { FlowNode, RunEvent } from "../types/flow";
import Console from "./Console";
import RunLog from "./RunLog";
import NodeChat, { type NodeChatState } from "./NodeChat";

export type BottomTab =
  | { kind: "console" }
  | { kind: "log" }
  | { kind: "executor"; nodeId: string; label: string };

interface BottomPanelProps {
  activeTab: BottomTab | null;
  onSelectTab: (tab: BottomTab | null) => void;
  height: number;
  onHeightChange: (h: number) => void;
  flowId: string | null;
  executorNodes: FlowNode[];
  // RunLog
  runEvents: RunEvent[];
  onRunEventsClear: () => void;
  // Node chat state
  nodeChatStates: Map<string, NodeChatState>;
  onNodeChatStateChange: (key: string, state: NodeChatState) => void;
  errorCount: number;
}

function tabKey(tab: BottomTab): string {
  if (tab.kind === "console") return "console";
  if (tab.kind === "log") return "log";
  return `exec:${tab.nodeId}`;
}

function tabsEqual(a: BottomTab, b: BottomTab): boolean {
  return tabKey(a) === tabKey(b);
}

const MIN_HEIGHT = 120;
const MAX_HEIGHT_RATIO = 0.8;

export default function BottomPanel({
  activeTab,
  onSelectTab,
  height,
  onHeightChange,
  flowId,
  executorNodes,
  runEvents,
  onRunEventsClear,
  nodeChatStates,
  onNodeChatStateChange,
  errorCount,
}: BottomPanelProps) {
  const dragRef = useRef<{ startY: number; startH: number } | null>(null);

  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = { startY: e.clientY, startH: height };

      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        const delta = dragRef.current.startY - ev.clientY;
        const maxH = Math.floor(window.innerHeight * MAX_HEIGHT_RATIO);
        const newH = Math.min(maxH, Math.max(MIN_HEIGHT, dragRef.current.startH + delta));
        onHeightChange(newH);
      };

      const onUp = () => {
        dragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };

      document.body.style.cursor = "ns-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [height, onHeightChange]
  );

  // Build ordered tab list: Console, Log, then executor nodes
  const tabs: BottomTab[] = useMemo(() => {
    const list: BottomTab[] = [
      { kind: "console" },
      { kind: "log" },
    ];
    executorNodes.forEach((node, i) => {
      list.push({
        kind: "executor",
        nodeId: node.id,
        label: node.label || `Executor ${i + 1}`,
      });
    });
    return list;
  }, [executorNodes]);

  if (!activeTab) return null;

  const handleClose = () => onSelectTab(null);

  return (
    <div className="bottom-panel" style={{ height }}>
      <div className="bottom-panel-drag-handle" onMouseDown={handleDragStart} />
      <div className="bottom-panel-tabs">
        {tabs.map((tab) => {
          const key = tabKey(tab);
          const isActive = tabsEqual(tab, activeTab);
          let label = "";
          if (tab.kind === "console") label = "Console";
          else if (tab.kind === "log") label = "Log";
          else label = tab.label;

          return (
            <button
              key={key}
              className={`bottom-panel-tab${isActive ? " active" : ""}${tab.kind === "executor" ? " executor" : ""}`}
              onClick={() => onSelectTab(tab)}
            >
              {label}
              {tab.kind === "console" && errorCount > 0 && (
                <span className="bottom-tab-badge">{errorCount}</span>
              )}
            </button>
          );
        })}
        <div style={{ flex: 1 }} />
        <button className="ghost bottom-panel-close" onClick={handleClose}>
          {"\u00d7"}
        </button>
      </div>

      <div className="bottom-panel-content">
        {activeTab.kind === "console" && (
          <Console onClose={handleClose} />
        )}
        {activeTab.kind === "log" && (
          <RunLog events={runEvents} onClear={onRunEventsClear} onClose={handleClose} />
        )}
        {activeTab.kind === "executor" && flowId && (
          <NodeChat
            key={`${flowId}::${activeTab.nodeId}`}
            flowId={flowId}
            nodeId={activeTab.nodeId}
            nodeLabel={activeTab.label}
            initialState={nodeChatStates.get(`${flowId}::${activeTab.nodeId}`) ?? null}
            onStateChange={(state) =>
              onNodeChatStateChange(`${flowId}::${activeTab.nodeId}`, state)
            }
          />
        )}
      </div>
    </div>
  );
}
