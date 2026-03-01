"use client";

import { useEffect, useRef, useState } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  Background,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { demoNodeTypes } from "./FlowDemoNodes";

const commonNode = { draggable: false, selectable: false } as const;

// Horizontal layout (desktop)
const desktopNodes: Node[] = [
  { id: "1", type: "demo", position: { x: 0, y: 80 }, data: { label: "Every 4 hours", type: "trigger", icon: "\u23f0" }, ...commonNode },
  { id: "2", type: "demo", position: { x: 240, y: 0 }, data: { label: "RSS Feeds", type: "source", icon: "\ud83d\udce1" }, ...commonNode },
  { id: "3", type: "demo", position: { x: 240, y: 160 }, data: { label: "Web Scraper", type: "source", icon: "\ud83c\udf10" }, ...commonNode },
  { id: "4", type: "demo", position: { x: 500, y: 80 }, data: { label: "Agent", type: "executor", icon: "\ud83e\udde0" }, ...commonNode },
  { id: "5", type: "demo", position: { x: 740, y: 0 }, data: { label: "Slack", type: "sink", icon: "\ud83d\udcac" }, ...commonNode },
  { id: "6", type: "demo", position: { x: 740, y: 160 }, data: { label: "Notion", type: "sink", icon: "\ud83d\udcdd" }, ...commonNode },
];

// Vertical branching layout (mobile)
const mobileNodes: Node[] = [
  { id: "1", type: "demoVertical", position: { x: 80, y: 0 }, data: { label: "Every 4 hours", type: "trigger", icon: "\u23f0" }, ...commonNode },
  { id: "2", type: "demoVertical", position: { x: 0, y: 130 }, data: { label: "RSS Feeds", type: "source", icon: "\ud83d\udce1" }, ...commonNode },
  { id: "3", type: "demoVertical", position: { x: 160, y: 130 }, data: { label: "Web Scraper", type: "source", icon: "\ud83c\udf10" }, ...commonNode },
  { id: "4", type: "demoVertical", position: { x: 80, y: 260 }, data: { label: "Agent", type: "executor", icon: "\ud83e\udde0" }, ...commonNode },
  { id: "5", type: "demoVertical", position: { x: 0, y: 390 }, data: { label: "Slack", type: "sink", icon: "\ud83d\udcac" }, ...commonNode },
  { id: "6", type: "demoVertical", position: { x: 160, y: 390 }, data: { label: "Notion", type: "sink", icon: "\ud83d\udcdd" }, ...commonNode },
];

const edgeStyle = { strokeWidth: 2 };

const desktopEdges: Edge[] = [
  { id: "e1-2", source: "1", target: "2", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-trigger", style: edgeStyle },
  { id: "e1-3", source: "1", target: "3", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-trigger", style: edgeStyle },
  { id: "e2-4", source: "2", target: "4", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-source", style: edgeStyle },
  { id: "e3-4", source: "3", target: "4", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-source", style: edgeStyle },
  { id: "e4-5", source: "4", target: "5", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-executor", style: edgeStyle },
  { id: "e4-6", source: "4", target: "6", sourceHandle: "out", targetHandle: "in", animated: true, className: "edge-executor", style: edgeStyle },
];

const mobileEdges: Edge[] = [
  { id: "e1-2", source: "1", target: "2", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-trigger", style: edgeStyle },
  { id: "e1-3", source: "1", target: "3", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-trigger", style: edgeStyle },
  { id: "e2-4", source: "2", target: "4", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-source", style: edgeStyle },
  { id: "e3-4", source: "3", target: "4", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-source", style: edgeStyle },
  { id: "e4-5", source: "4", target: "5", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-executor", style: edgeStyle },
  { id: "e4-6", source: "4", target: "6", sourceHandle: "out-bottom", targetHandle: "in-top", animated: true, className: "edge-executor", style: edgeStyle },
];

function FlowInner({ isMobile }: { isMobile: boolean }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const { fitView } = useReactFlow();

  // Re-fit when layout direction changes
  useEffect(() => {
    const timer = setTimeout(() => fitView({ padding: 0.2 }), 50);
    return () => clearTimeout(timer);
  }, [isMobile, fitView]);

  // Re-fit on container resize
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let timer: ReturnType<typeof setTimeout>;
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => fitView({ padding: 0.2 }), 50);
    });

    observer.observe(container);
    return () => {
      observer.disconnect();
      clearTimeout(timer);
    };
  }, [fitView]);

  return (
    <div
      ref={containerRef}
      className="rounded-xl border border-border overflow-hidden"
      style={{ height: isMobile ? 580 : 320, background: "var(--bg)" }}
    >
      <ReactFlow
        nodes={isMobile ? mobileNodes : desktopNodes}
        edges={isMobile ? mobileEdges : desktopEdges}
        nodeTypes={demoNodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        proOptions={{ hideAttribution: true }}
        panOnDrag={false}
        zoomOnScroll={false}
        zoomOnPinch={false}
        zoomOnDoubleClick={false}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        preventScrolling={false}
        onInit={() => fitView({ padding: 0.2 })}
      >
        <Background color="var(--bg-tertiary)" gap={20} size={1} />
      </ReactFlow>
    </div>
  );
}

export default function FlowDemo() {
  const [isMobile, setIsMobile] = useState(false);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 640px)");
    setIsMobile(mq.matches);
    const handler = (e: MediaQueryListEvent) => setIsMobile(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  return (
    <ReactFlowProvider>
      <FlowInner isMobile={isMobile} />
    </ReactFlowProvider>
  );
}
