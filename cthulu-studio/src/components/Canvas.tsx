import { useCallback, useRef, useEffect, forwardRef, useImperativeHandle } from "react";
import {
  ReactFlow,
  MiniMap,
  Controls,
  Background,
  BackgroundVariant,
  Position,
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
import FilterNode from "./NodeTypes/FilterNode";
import SinkNode from "./NodeTypes/SinkNode";
import type { Flow, FlowNode, FlowEdge } from "../types/flow";
import { log } from "../api/logger";

const rfNodeTypes: NodeTypes = {
  trigger: TriggerNode,
  source: SourceNode,
  filter: FilterNode,
  executor: ExecutorNode,
  sink: SinkNode,
};

const EDGE_STYLE = { stroke: "#30363d", strokeWidth: 2 };

function toRFNodes(flow: Flow): RFNode[] {
  return flow.nodes.map((n) => ({
    id: n.id,
    type: n.node_type,
    position: { x: n.position.x, y: n.position.y },
    data: { label: n.label, kind: n.kind, config: n.config },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
  }));
}

function toRFEdges(flow: Flow): RFEdge[] {
  return flow.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: "out",
    targetHandle: "in",
    type: "smoothstep",
    animated: true,
    style: EDGE_STYLE,
  }));
}

function toFlowNodes(rfNodes: RFNode[]): FlowNode[] {
  return rfNodes.map((n) => ({
    id: n.id,
    node_type: n.type as FlowNode["node_type"],
    kind: n.data.kind as string,
    config: n.data.config as Record<string, unknown>,
    position: { x: n.position.x, y: n.position.y },
    label: n.data.label as string,
  }));
}

function toFlowEdges(rfEdges: RFEdge[]): FlowEdge[] {
  return rfEdges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
  }));
}

// Allowed: trigger→source, trigger→executor, source→executor, executor→sink
const VALID_TARGETS: Record<string, string[]> = {
  trigger: ["source", "executor"],
  source: ["executor"],
  executor: ["sink"],
};

export interface CanvasHandle {
  addNodeAtScreen: (
    nodeType: string,
    kind: string,
    label: string,
    screenX: number,
    screenY: number
  ) => FlowNode | null;
  getNode: (id: string) => FlowNode | null;
  updateNodeData: (id: string, updates: { label?: string; config?: Record<string, unknown> }) => void;
  deleteNode: (id: string) => void;
}

interface CanvasProps {
  flowId: string | null;
  initialFlow: Flow | null;
  onFlowSnapshot: (snapshot: { nodes: FlowNode[]; edges: FlowEdge[] }) => void;
  onSelectionChange: (nodeId: string | null) => void;
  nodeRunStatus?: Record<string, "running" | "completed" | "failed">;
}

const Canvas = forwardRef<CanvasHandle, CanvasProps>(function Canvas(
  { flowId, initialFlow, onFlowSnapshot, onSelectionChange, nodeRunStatus },
  ref
) {
  const [nodes, setNodes, onNodesChange] = useNodesState<RFNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<RFEdge>([]);
  const rfInstance = useRef<any>(null);
  const prevFlowId = useRef<string | null>(null);
  const nodesRef = useRef<RFNode[]>(nodes);
  nodesRef.current = nodes;

  // --- Seed RF state on flow switch ---
  useEffect(() => {
    if (flowId === prevFlowId.current) return;
    prevFlowId.current = flowId;
    if (initialFlow && initialFlow.id === flowId) {
      setNodes(toRFNodes(initialFlow));
      setEdges(toRFEdges(initialFlow));
    } else {
      setNodes([]);
      setEdges([]);
    }
  }, [flowId, initialFlow, setNodes, setEdges]);

  // --- Merge run status into node data ---
  useEffect(() => {
    if (!nodeRunStatus) return;
    setNodes((nds) =>
      nds.map((n) => {
        const status = nodeRunStatus[n.id] ?? null;
        if (n.data.runStatus === status) return n;
        return { ...n, data: { ...n.data, runStatus: status } };
      })
    );
  }, [nodeRunStatus, setNodes]);

  // --- Snapshot emission (debounced) ---
  useEffect(() => {
    if (!flowId) return;
    const timer = setTimeout(() => {
      onFlowSnapshot({ nodes: toFlowNodes(nodes), edges: toFlowEdges(edges) });
    }, 300);
    return () => clearTimeout(timer);
  }, [nodes, edges, flowId, onFlowSnapshot]);

  // --- Imperative handle ---
  useImperativeHandle(ref, () => ({
    addNodeAtScreen(nodeType, kind, label, screenX, screenY) {
      if (!rfInstance.current) return null;
      const position = rfInstance.current.screenToFlowPosition({ x: screenX, y: screenY });

      const newNode: RFNode = {
        id: crypto.randomUUID(),
        type: nodeType,
        position,
        data: { label, kind, config: {} },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
      };

      setNodes((nds) => [...nds, newNode]);
      onSelectionChange(newNode.id);

      return {
        id: newNode.id,
        node_type: nodeType as FlowNode["node_type"],
        kind,
        config: {},
        position: { x: position.x, y: position.y },
        label,
      };
    },

    getNode(id) {
      const rfNode = nodes.find((n) => n.id === id);
      if (!rfNode) return null;
      return {
        id: rfNode.id,
        node_type: rfNode.type as FlowNode["node_type"],
        kind: rfNode.data.kind as string,
        config: rfNode.data.config as Record<string, unknown>,
        position: { x: rfNode.position.x, y: rfNode.position.y },
        label: rfNode.data.label as string,
      };
    },

    updateNodeData(id, updates) {
      setNodes((nds) =>
        nds.map((n) => {
          if (n.id !== id) return n;
          return {
            ...n,
            data: {
              ...n.data,
              ...(updates.label !== undefined ? { label: updates.label } : {}),
              ...(updates.config !== undefined ? { config: updates.config } : {}),
            },
          };
        })
      );
    },

    deleteNode(id) {
      setNodes((nds) => nds.filter((n) => n.id !== id));
      setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
      onSelectionChange(null);
    },
  }), [nodes, setNodes, setEdges, onSelectionChange]);

  // --- RF event handlers ---
  const handleConnect = useCallback(
    (params: Connection) => {
      if (!params.source || !params.target) return;

      const currentNodes = nodesRef.current;
      const srcNode = currentNodes.find((n) => n.id === params.source);
      const tgtNode = currentNodes.find((n) => n.id === params.target);
      if (!srcNode?.type || !tgtNode?.type) return;

      const allowed = VALID_TARGETS[srcNode.type];
      if (!allowed?.includes(tgtNode.type)) {
        log("warn", "[Canvas] Invalid connection", `${srcNode.type} → ${tgtNode.type}`);
        return;
      }

      setEdges((eds) =>
        addEdge(
          { ...params, type: "smoothstep", animated: true, style: EDGE_STYLE },
          eds
        )
      );
    },
    [setEdges]
  );

  const handleNodesDelete = useCallback(
    (deleted: RFNode[]) => {
      if (deleted.length > 0) {
        onSelectionChange(null);
      }
    },
    [onSelectionChange]
  );

  return (
    <div className="canvas-container">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={handleConnect}
        onNodesDelete={handleNodesDelete}
        onNodeClick={(_e, node) => onSelectionChange(node.id)}
        onPaneClick={() => onSelectionChange(null)}
        onInit={(instance) => { rfInstance.current = instance; }}
        nodeTypes={rfNodeTypes}
        defaultEdgeOptions={{ type: "smoothstep", animated: true, style: EDGE_STYLE }}
        fitView
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="#21262d" />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            switch (node.type) {
              case "trigger": return "#d29922";
              case "source": return "#58a6ff";
              case "executor": return "#bc8cff";
              case "sink": return "#3fb950";
              default: return "#30363d";
            }
          }}
          maskColor="rgba(0,0,0,0.5)"
        />
      </ReactFlow>
    </div>
  );
});

export default Canvas;
