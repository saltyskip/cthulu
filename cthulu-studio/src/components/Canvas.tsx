import { useCallback, useRef } from "react";
import {
  ReactFlow,
  MiniMap,
  Controls,
  Background,
  BackgroundVariant,
  useNodesState,
  useEdgesState,
  addEdge,
  type Connection,
  type Node as RFNode,
  type Edge as RFEdge,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import TriggerNode from "./NodeTypes/TriggerNode";
import SourceNode from "./NodeTypes/SourceNode";
import ExecutorNode from "./NodeTypes/ExecutorNode";
import SinkNode from "./NodeTypes/SinkNode";
import type { Flow, FlowNode, FlowEdge, NodeTypeSchema } from "../types/flow";

const nodeTypes: NodeTypes = {
  trigger: TriggerNode,
  source: SourceNode,
  executor: ExecutorNode,
  sink: SinkNode,
};

function flowToRFNodes(flow: Flow): RFNode[] {
  return flow.nodes.map((n) => ({
    id: n.id,
    type: n.node_type,
    position: { x: n.position.x, y: n.position.y },
    data: { label: n.label, kind: n.kind, config: n.config },
  }));
}

function flowToRFEdges(flow: Flow): RFEdge[] {
  return flow.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    animated: true,
    style: { stroke: "#30363d", strokeWidth: 2 },
  }));
}

interface CanvasProps {
  flow: Flow;
  onNodesChange: (nodes: FlowNode[]) => void;
  onEdgesChange: (edges: FlowEdge[]) => void;
  onNodeSelect: (nodeId: string | null) => void;
  onDrop: (nodeType: NodeTypeSchema, position: { x: number; y: number }) => void;
}

export default function Canvas({
  flow,
  onNodesChange: onFlowNodesChange,
  onEdgesChange: onFlowEdgesChange,
  onNodeSelect,
  onDrop,
}: CanvasProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const [nodes, setNodes, onNodesChange] = useNodesState(flowToRFNodes(flow));
  const [edges, setEdges, onEdgesChange] = useEdgesState(flowToRFEdges(flow));
  const reactFlowInstance = useRef<any>(null);

  // Build a fingerprint of flow structure + data (excludes positions to avoid drag loops)
  const flowFingerprint = `${flow.id}:${flow.nodes.length}:${flow.edges.length}:` +
    flow.nodes.map((n) => `${n.id}|${n.label}|${n.kind}|${JSON.stringify(n.config)}`).join(",");

  const prevFingerprint = useRef(flowFingerprint);

  if (flowFingerprint !== prevFingerprint.current) {
    prevFingerprint.current = flowFingerprint;
    setNodes(flowToRFNodes(flow));
    setEdges(flowToRFEdges(flow));
  }

  const onConnect = useCallback(
    (params: Connection) => {
      setEdges((eds) => {
        const newEdges = addEdge(
          { ...params, animated: true, style: { stroke: "#30363d", strokeWidth: 2 } },
          eds
        );
        syncEdges(newEdges);
        return newEdges;
      });
    },
    [setEdges]
  );

  const syncNodes = useCallback(
    (rfNodes: RFNode[]) => {
      const flowNodes: FlowNode[] = rfNodes.map((n) => ({
        id: n.id,
        node_type: n.type as FlowNode["node_type"],
        kind: n.data.kind as string,
        config: n.data.config as Record<string, unknown>,
        position: { x: n.position.x, y: n.position.y },
        label: n.data.label as string,
      }));
      onFlowNodesChange(flowNodes);
    },
    [onFlowNodesChange]
  );

  const syncEdges = useCallback(
    (rfEdges: RFEdge[]) => {
      const flowEdges: FlowEdge[] = rfEdges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
      }));
      onFlowEdgesChange(flowEdges);
    },
    [onFlowEdgesChange]
  );

  const handleNodesChange = useCallback(
    (changes: any) => {
      onNodesChange(changes);
      // Debounced sync would be ideal, but for now sync on change
      setNodes((nds) => {
        // Schedule a sync after state update
        setTimeout(() => syncNodes(nds), 0);
        return nds;
      });
    },
    [onNodesChange, setNodes, syncNodes]
  );

  const handleEdgesChange = useCallback(
    (changes: any) => {
      onEdgesChange(changes);
      setEdges((eds) => {
        setTimeout(() => syncEdges(eds), 0);
        return eds;
      });
    },
    [onEdgesChange, setEdges, syncEdges]
  );

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: RFNode) => {
      onNodeSelect(node.id);
    },
    [onNodeSelect]
  );

  const handlePaneClick = useCallback(() => {
    onNodeSelect(null);
  }, [onNodeSelect]);

  const handleDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  }, []);

  const handleDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const data = event.dataTransfer.getData("application/cthulu-node");
      if (!data) return;

      const nodeType: NodeTypeSchema = JSON.parse(data);
      const bounds = reactFlowWrapper.current?.getBoundingClientRect();
      if (!bounds || !reactFlowInstance.current) return;

      const position = reactFlowInstance.current.screenToFlowPosition({
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });

      onDrop(nodeType, position);
    },
    [onDrop]
  );

  return (
    <div className="canvas-container" ref={reactFlowWrapper}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        onInit={(instance) => {
          reactFlowInstance.current = instance;
        }}
        nodeTypes={nodeTypes}
        fitView
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="#21262d" />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            switch (node.type) {
              case "trigger":
                return "#d29922";
              case "source":
                return "#58a6ff";
              case "executor":
                return "#bc8cff";
              case "sink":
                return "#3fb950";
              default:
                return "#30363d";
            }
          }}
          maskColor="rgba(0,0,0,0.5)"
        />
      </ReactFlow>
    </div>
  );
}
