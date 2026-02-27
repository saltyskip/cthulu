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
import SinkNode from "./NodeTypes/SinkNode";
import type { Flow, FlowNode, FlowEdge } from "../types/flow";
import type { UpdateSignal } from "../hooks/useFlowDispatch";
import { log } from "../api/logger";

const rfNodeTypes: NodeTypes = {
  trigger: TriggerNode,
  source: SourceNode,
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
  /** Spread-merge nodes/edges from an external source (e.g. JSON editor). */
  mergeFromFlow: (nodes: FlowNode[], edges: FlowEdge[]) => void;
}

interface CanvasProps {
  flowId: string | null;
  canonicalFlow: Flow | null;
  updateSignal: UpdateSignal;
  onFlowChange: (updates: { nodes: FlowNode[]; edges: FlowEdge[] }) => void;
  onSelectionChange: (nodeId: string | null) => void;
  nodeRunStatus?: Record<string, "running" | "completed" | "failed">;
  nodeValidationErrors?: Record<string, string[]>;
}

const Canvas = forwardRef<CanvasHandle, CanvasProps>(function Canvas(
  { flowId, canonicalFlow, updateSignal, onFlowChange, onSelectionChange, nodeRunStatus, nodeValidationErrors },
  ref
) {
  const [nodes, setNodes, onNodesChange] = useNodesState<RFNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<RFEdge>([]);
  const rfInstance = useRef<any>(null);
  const prevFlowId = useRef<string | null>(null);
  const nodesRef = useRef<RFNode[]>(nodes);
  nodesRef.current = nodes;
  const edgesRef = useRef<RFEdge[]>(edges);
  edgesRef.current = edges;

  const lastAppliedCounter = useRef(0);

  // --- Notify parent of changes (replaces scheduleSave) ---
  const onFlowChangeRef = useRef(onFlowChange);
  onFlowChangeRef.current = onFlowChange;

  const notifyChange = useCallback(() => {
    onFlowChangeRef.current({
      nodes: toFlowNodes(nodesRef.current),
      edges: toFlowEdges(edgesRef.current),
    });
  }, []);

  // --- Spread-merge from external flow data ---
  const mergeFromFlowInternal = useCallback((flowNodes: FlowNode[], flowEdges: FlowEdge[]) => {
    setNodes((prev) => {
      const prevMap = new Map(prev.map((n) => [n.id, n]));
      return flowNodes.map((fn) => {
        const existing = prevMap.get(fn.id);
        if (existing) {
          return {
            ...existing,
            position: { x: fn.position.x, y: fn.position.y },
            data: { ...existing.data, label: fn.label, kind: fn.kind, config: fn.config },
          };
        }
        return {
          id: fn.id,
          type: fn.node_type,
          position: { x: fn.position.x, y: fn.position.y },
          data: { label: fn.label, kind: fn.kind, config: fn.config },
          sourcePosition: Position.Right,
          targetPosition: Position.Left,
        };
      });
    });
    setEdges(flowEdges.map((fe) => ({
      id: fe.id,
      source: fe.source,
      target: fe.target,
      sourceHandle: "out",
      targetHandle: "in",
      type: "smoothstep",
      animated: true,
      style: EDGE_STYLE,
    })));
  }, [setNodes, setEdges]);

  // --- Seed RF state on flow switch ---
  useEffect(() => {
    if (flowId === prevFlowId.current) return;
    prevFlowId.current = flowId;
    lastAppliedCounter.current = updateSignal.counter;
    if (canonicalFlow && canonicalFlow.id === flowId) {
      setNodes(toRFNodes(canonicalFlow));
      setEdges(toRFEdges(canonicalFlow));
    } else {
      setNodes([]);
      setEdges([]);
    }
  }, [flowId, canonicalFlow, updateSignal.counter, setNodes, setEdges]);

  // --- Apply external updates via updateSignal ---
  useEffect(() => {
    if (updateSignal.counter <= lastAppliedCounter.current) return;
    lastAppliedCounter.current = updateSignal.counter;
    if (updateSignal.source === "canvas") return; // we originated this
    if (!canonicalFlow) return;
    mergeFromFlowInternal(canonicalFlow.nodes, canonicalFlow.edges);
  }, [updateSignal, canonicalFlow, mergeFromFlowInternal]);

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

  // --- Merge validation errors into node data ---
  useEffect(() => {
    if (!nodeValidationErrors) return;
    setNodes((nds) =>
      nds.map((n) => {
        const errors = nodeValidationErrors[n.id] ?? undefined;
        const prev = n.data.validationErrors as string[] | undefined;
        if (prev === errors || (JSON.stringify(prev) === JSON.stringify(errors))) return n;
        return { ...n, data: { ...n.data, validationErrors: errors } };
      })
    );
  }, [nodeValidationErrors, setNodes]);

  // --- Imperative handle ---
  useImperativeHandle(ref, () => ({
    addNodeAtScreen(nodeType, kind, label, screenX, screenY) {
      if (!rfInstance.current) return null;
      const position = rfInstance.current.screenToFlowPosition({ x: screenX, y: screenY });

      // Auto-name executor nodes: Executor - E01, E02, ...
      let finalLabel = label;
      if (nodeType === "executor") {
        const existingCount = nodes.filter((n) => n.type === "executor").length;
        finalLabel = `Executor - E${String(existingCount + 1).padStart(2, "0")}`;
      }

      const defaultConfig = nodeType === "executor" ? { agent_id: "" } : {};

      const newNode: RFNode = {
        id: crypto.randomUUID(),
        type: nodeType,
        position,
        data: { label: finalLabel, kind, config: defaultConfig },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
      };

      setNodes((nds) => [...nds, newNode]);
      onSelectionChange(newNode.id);
      // setNodes hasn't flushed yet, use setTimeout so nodesRef is up to date
      setTimeout(() => notifyChange(), 0);

      return {
        id: newNode.id,
        node_type: nodeType as FlowNode["node_type"],
        kind,
        config: defaultConfig,
        position: { x: position.x, y: position.y },
        label: finalLabel,
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
      setTimeout(() => notifyChange(), 0);
    },

    deleteNode(id) {
      setNodes((nds) => nds.filter((n) => n.id !== id));
      setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
      onSelectionChange(null);
      setTimeout(() => notifyChange(), 0);
    },

    mergeFromFlow(flowNodes, flowEdges) {
      mergeFromFlowInternal(flowNodes, flowEdges);
    },
  }), [nodes, setNodes, setEdges, onSelectionChange, notifyChange, mergeFromFlowInternal]);

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
      setTimeout(() => notifyChange(), 0);
    },
    [setEdges, notifyChange]
  );

  const handleNodesDelete = useCallback(
    (deleted: RFNode[]) => {
      if (deleted.length > 0) {
        onSelectionChange(null);
        setTimeout(() => notifyChange(), 0);
      }
    },
    [onSelectionChange, notifyChange]
  );

  const handleNodeDragStop = useCallback(() => {
    notifyChange();
  }, [notifyChange]);

  const handleEdgesDelete = useCallback(() => {
    setTimeout(() => notifyChange(), 0);
  }, [notifyChange]);

  return (
    <div className="canvas-container">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={handleConnect}
        onNodesDelete={handleNodesDelete}
        onNodeDragStop={handleNodeDragStop}
        onEdgesDelete={handleEdgesDelete}
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
