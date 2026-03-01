/**
 * MiniFlowDiagram — a small read-only React Flow graph that visualises the
 * trigger → sources → filters → executors → sinks pipeline shape of a template.
 *
 * Used inside TemplateGallery cards on hover.
 */
import { useMemo } from "react";
import {
  ReactFlow,
  type Node as RFNode,
  type Edge as RFEdge,
  Background,
  BackgroundVariant,
} from "@xyflow/react";
import type { PipelineShape } from "../types/flow";

interface MiniFlowDiagramProps {
  shape: PipelineShape;
}

// Node colour mapping — matches the existing Studio colour system
const NODE_COLORS: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

const X_STEP = 110;
const Y_CENTER = 40;

function makeMiniNode(
  id: string,
  label: string,
  kind: string,
  x: number,
  y: number
): RFNode {
  return {
    id,
    type: "default",
    position: { x, y },
    data: { label },
    style: {
      background: NODE_COLORS[kind] ?? "var(--bg-tertiary)",
      color: "var(--primary-foreground)",
      border: "none",
      borderRadius: 6,
      padding: "3px 7px",
      fontSize: 9,
      fontWeight: 600,
      minWidth: 60,
      maxWidth: 90,
      textAlign: "center",
      whiteSpace: "nowrap",
      overflow: "hidden",
      textOverflow: "ellipsis",
      boxShadow: `0 0 0 1px ${NODE_COLORS[kind] ?? "var(--border)"}44`,
    },
    draggable: false,
    selectable: false,
    connectable: false,
  };
}

function makeMiniEdge(source: string, target: string): RFEdge {
  return {
    id: `e-${source}-${target}`,
    source,
    target,
    animated: false,
    style: { stroke: "var(--border, #30363d)", strokeWidth: 1.5 },
  };
}

function kindLabel(kind: string): string {
  const labels: Record<string, string> = {
    cron: "Cron",
    manual: "Manual",
    "github-pr": "GitHub PR",
    webhook: "Webhook",
    rss: "RSS",
    "web-scrape": "Web Scrape",
    "web-scraper": "Scraper",
    "github-merged-prs": "GitHub PRs",
    "market-data": "Market",
    keyword: "Filter",
    "claude-code": "Claude",
    "vm-sandbox": "VM",
    slack: "Slack",
    notion: "Notion",
  };
  return labels[kind] ?? kind;
}

export default function MiniFlowDiagram({ shape }: MiniFlowDiagramProps) {
  const { nodes, edges } = useMemo(() => {
    const rfNodes: RFNode[] = [];
    const rfEdges: RFEdge[] = [];
    let x = 0;

    // --- Trigger ---
    const triggerId = "t0";
    rfNodes.push(
      makeMiniNode(triggerId, kindLabel(shape.trigger), "trigger", x, Y_CENTER)
    );
    x += X_STEP;

    // --- Sources ---
    const sourceIds: string[] = [];
    if (shape.sources.length > 0) {
      const spreadY = (shape.sources.length - 1) * 30;
      shape.sources.forEach((kind, i) => {
        const id = `s${i}`;
        const y =
          shape.sources.length === 1
            ? Y_CENTER
            : Y_CENTER - spreadY / 2 + i * 30;
        rfNodes.push(makeMiniNode(id, kindLabel(kind), "source", x, y));
        sourceIds.push(id);
        rfEdges.push(makeMiniEdge(triggerId, id));
      });
      x += X_STEP;
    }

    // --- Executors ---
    const executorIds: string[] = [];
    shape.executors.forEach((kind, i) => {
      const id = `e${i}`;
      rfNodes.push(makeMiniNode(id, kindLabel(kind), "executor", x, Y_CENTER));
      executorIds.push(id);
      if (i === 0) {
        // Connect previous stage
        const prevGroup =
          sourceIds.length > 0
            ? sourceIds
            : [triggerId];
        prevGroup.forEach((pid) => rfEdges.push(makeMiniEdge(pid, id)));
      } else {
        // Chain executors
        rfEdges.push(makeMiniEdge(executorIds[i - 1], id));
      }
      x += X_STEP;
    });

    // --- Sinks ---
    if (shape.sinks.length > 0) {
      const spreadY = (shape.sinks.length - 1) * 30;
      shape.sinks.forEach((kind, i) => {
        const id = `k${i}`;
        const y =
          shape.sinks.length === 1
            ? Y_CENTER
            : Y_CENTER - spreadY / 2 + i * 30;
        rfNodes.push(makeMiniNode(id, kindLabel(kind), "sink", x, y));
        const lastExec = executorIds[executorIds.length - 1];
        if (lastExec) rfEdges.push(makeMiniEdge(lastExec, id));
      });
    }

    return { nodes: rfNodes, edges: rfEdges };
  }, [shape]);

  return (
    <div className="mini-flow-container">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        fitView
        fitViewOptions={{ padding: 0.3 }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        panOnDrag={false}
        zoomOnScroll={false}
        zoomOnPinch={false}
        zoomOnDoubleClick={false}
        preventScrolling={false}
        proOptions={{ hideAttribution: true }}
        style={{ background: "transparent" }}
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={14}
          size={0.8}
          color="var(--border, #30363d)"
        />
      </ReactFlow>
    </div>
  );
}
