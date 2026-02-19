"use client";

import {
  ReactFlow,
  Background,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { demoNodeTypes } from "./FlowDemoNodes";

const nodes: Node[] = [
  {
    id: "1",
    type: "demo",
    position: { x: 0, y: 80 },
    data: { label: "Every 4 hours", type: "trigger", icon: "\u23f0" },
    draggable: false,
    selectable: false,
  },
  {
    id: "2",
    type: "demo",
    position: { x: 240, y: 0 },
    data: { label: "RSS Feeds", type: "source", icon: "\ud83d\udce1" },
    draggable: false,
    selectable: false,
  },
  {
    id: "3",
    type: "demo",
    position: { x: 240, y: 160 },
    data: { label: "Web Scraper", type: "source", icon: "\ud83c\udf10" },
    draggable: false,
    selectable: false,
  },
  {
    id: "4",
    type: "demo",
    position: { x: 500, y: 80 },
    data: { label: "Agent", type: "executor", icon: "\ud83e\udde0" },
    draggable: false,
    selectable: false,
  },
  {
    id: "5",
    type: "demo",
    position: { x: 740, y: 0 },
    data: { label: "Slack", type: "sink", icon: "\ud83d\udcac" },
    draggable: false,
    selectable: false,
  },
  {
    id: "6",
    type: "demo",
    position: { x: 740, y: 160 },
    data: { label: "Notion", type: "sink", icon: "\ud83d\udcdd" },
    draggable: false,
    selectable: false,
  },
];

const edges: Edge[] = [
  {
    id: "e1-2",
    source: "1",
    target: "2",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#d29922", strokeWidth: 2 },
  },
  {
    id: "e1-3",
    source: "1",
    target: "3",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#d29922", strokeWidth: 2 },
  },
  {
    id: "e2-4",
    source: "2",
    target: "4",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#58a6ff", strokeWidth: 2 },
  },
  {
    id: "e3-4",
    source: "3",
    target: "4",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#58a6ff", strokeWidth: 2 },
  },
  {
    id: "e4-5",
    source: "4",
    target: "5",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#bc8cff", strokeWidth: 2 },
  },
  {
    id: "e4-6",
    source: "4",
    target: "6",
    sourceHandle: "out",
    targetHandle: "in",
    animated: true,
    style: { stroke: "#bc8cff", strokeWidth: 2 },
  },
];

export default function FlowDemo() {
  return (
    <div
      className="rounded-xl border border-border overflow-hidden"
      style={{ height: 320, background: "#0d1117" }}
    >
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={demoNodeTypes}
        fitView
        fitViewOptions={{ padding: 0.3 }}
        proOptions={{ hideAttribution: true }}
        panOnDrag={false}
        zoomOnScroll={false}
        zoomOnPinch={false}
        zoomOnDoubleClick={false}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        preventScrolling={false}
      >
        <Background color="#21262d" gap={20} size={1} />
      </ReactFlow>
    </div>
  );
}
