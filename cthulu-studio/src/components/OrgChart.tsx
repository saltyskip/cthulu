import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Users } from "lucide-react";
import { listAgents } from "../api/client";
import { useNavigation } from "../contexts/NavigationContext";
import type { AgentSummary, AgentRole } from "../types/flow";
import { ROLE_LABELS, STUDIO_ASSISTANT_ID } from "../types/flow";

// ── Layout constants ──────────────────────────────────────────────────
const CARD_WIDTH = 200;
const CARD_HEIGHT = 80;
const H_GAP = 40;
const V_GAP = 100;

// ── Tree node ─────────────────────────────────────────────────────────
interface OrgNode {
  agent: AgentSummary;
  children: OrgNode[];
  x: number;
  y: number;
  width: number; // subtree width
}

// ── Tree helpers ──────────────────────────────────────────────────────

function buildForest(agents: AgentSummary[]): OrgNode[] {
  const eligible = agents.filter(
    (a) => a.id !== STUDIO_ASSISTANT_ID && !a.subagent_only,
  );

  const byId = new Map<string, OrgNode>();
  for (const agent of eligible) {
    byId.set(agent.id, { agent, children: [], x: 0, y: 0, width: 0 });
  }

  const roots: OrgNode[] = [];
  for (const node of byId.values()) {
    const parentId = node.agent.reports_to;
    if (parentId && byId.has(parentId)) {
      byId.get(parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  return roots;
}

function subtreeWidth(node: OrgNode): number {
  if (node.children.length === 0) {
    node.width = CARD_WIDTH;
    return CARD_WIDTH;
  }
  const total =
    node.children.reduce((sum, c) => sum + subtreeWidth(c), 0) +
    H_GAP * (node.children.length - 1);
  node.width = Math.max(CARD_WIDTH, total);
  return node.width;
}

function layoutTree(node: OrgNode, x: number, y: number): void {
  node.x = x + (node.width - CARD_WIDTH) / 2;
  node.y = y;

  let childX = x;
  const childY = y + CARD_HEIGHT + V_GAP;
  for (const child of node.children) {
    layoutTree(child, childX, childY);
    childX += child.width + H_GAP;
  }
}

function layoutForest(forest: OrgNode[]): {
  nodes: OrgNode[];
  totalWidth: number;
  totalHeight: number;
} {
  // compute widths
  for (const root of forest) subtreeWidth(root);

  let x = 0;
  for (const root of forest) {
    layoutTree(root, x, 0);
    x += root.width + H_GAP;
  }

  const totalWidth = x > 0 ? x - H_GAP : 0;

  // collect all nodes + find max depth
  const all: OrgNode[] = [];
  let maxY = 0;
  function collect(n: OrgNode) {
    all.push(n);
    if (n.y > maxY) maxY = n.y;
    for (const c of n.children) collect(c);
  }
  for (const root of forest) collect(root);

  return { nodes: all, totalWidth, totalHeight: maxY + CARD_HEIGHT };
}

// ── Collect edges ────────────────────────────────────────────────────
interface Edge {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

function collectEdges(forest: OrgNode[]): Edge[] {
  const edges: Edge[] = [];
  function walk(node: OrgNode) {
    for (const child of node.children) {
      edges.push({
        x1: node.x + CARD_WIDTH / 2,
        y1: node.y + CARD_HEIGHT,
        x2: child.x + CARD_WIDTH / 2,
        y2: child.y,
      });
      walk(child);
    }
  }
  for (const root of forest) walk(root);
  return edges;
}

// ── Component ────────────────────────────────────────────────────────

interface OrgChartProps {
  onSelectAgent?: (agentId: string) => void;
}

export default function OrgChart({ onSelectAgent }: OrgChartProps) {
  const { setActiveView, setSelectedAgentId } = useNavigation();

  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // pan / zoom state
  const [panX, setPanX] = useState(40);
  const [panY, setPanY] = useState(40);
  const [zoom, setZoom] = useState(1);

  // refs for pan dragging (avoid re-renders during drag)
  const containerRef = useRef<HTMLDivElement>(null);
  const isPanning = useRef(false);
  const panStart = useRef({ x: 0, y: 0, panX: 0, panY: 0 });

  // ── Fetch agents ──
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const data = await listAgents();
        if (!cancelled) {
          setAgents(data);
          setLoading(false);
        }
      } catch (e) {
        if (!cancelled) {
          const msg =
            typeof e === "string"
              ? e
              : e instanceof Error
                ? e.message
                : String(e);
          setError(msg);
          setLoading(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Build tree & layout (derived from agents) ──
  const { forest, allNodes, edges, totalWidth, totalHeight, hasAgents } =
    useMemo(() => {
      const forest = buildForest(agents);
      const hasAgents = forest.length > 0;
      const { nodes: allNodes, totalWidth, totalHeight } =
        layoutForest(forest);
      const edges = collectEdges(forest);
      return { forest, allNodes, edges, totalWidth, totalHeight, hasAgents };
    }, [agents]);

  // ── Navigate to agent detail ──
  const handleCardClick = useCallback(
    (agentId: string) => {
      if (onSelectAgent) {
        onSelectAgent(agentId);
      } else {
        setSelectedAgentId(agentId);
        setActiveView("agent-detail");
      }
    },
    [onSelectAgent, setSelectedAgentId, setActiveView],
  );

  // ── Pan handlers ──
  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      // only primary button
      if (e.button !== 0) return;
      isPanning.current = true;
      panStart.current = { x: e.clientX, y: e.clientY, panX, panY };
      e.preventDefault();
    },
    [panX, panY],
  );

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isPanning.current) return;
    const dx = e.clientX - panStart.current.x;
    const dy = e.clientY - panStart.current.y;
    setPanX(panStart.current.panX + dx);
    setPanY(panStart.current.panY + dy);
  }, []);

  const onMouseUp = useCallback(() => {
    isPanning.current = false;
  }, []);

  const onMouseLeave = useCallback(() => {
    isPanning.current = false;
  }, []);

  // ── Zoom handler ──
  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setZoom((prev) => {
      const delta = e.deltaY > 0 ? -0.05 : 0.05;
      return Math.min(2.0, Math.max(0.3, prev + delta));
    });
  }, []);

  // ── Loading ──
  if (loading) {
    return (
      <div className="org-chart-empty">
        <p>Loading org chart...</p>
      </div>
    );
  }

  // ── Error ──
  if (error) {
    return (
      <div className="org-chart-empty">
        <p>Error: {error}</p>
      </div>
    );
  }

  // ── Empty state ──
  if (!hasAgents) {
    return (
      <div className="org-chart-empty">
        <Users size={48} />
        <p>
          No agents found. Create agents and set &lsquo;Reports To&rsquo; in
          their Configuration tab to build the org chart.
        </p>
      </div>
    );
  }

  // ── SVG dimensions (with padding) ──
  const svgW = totalWidth + 200;
  const svgH = totalHeight + 200;

  return (
    <div
      ref={containerRef}
      className="org-chart-container"
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={onMouseLeave}
      onWheel={onWheel}
    >
      <div
        className="org-chart-inner"
        style={{
          transform: `translate(${panX}px, ${panY}px) scale(${zoom})`,
          width: svgW,
          height: svgH,
        }}
      >
        {/* SVG edge layer */}
        <svg
          className="org-chart-edges"
          width={svgW}
          height={svgH}
        >
          {edges.map((edge, i) => {
            const midY = (edge.y1 + edge.y2) / 2;
            const d = `M ${edge.x1} ${edge.y1} C ${edge.x1} ${midY}, ${edge.x2} ${midY}, ${edge.x2} ${edge.y2}`;
            return (
              <path
                key={i}
                d={d}
                fill="none"
                stroke="var(--border)"
                strokeWidth={2}
              />
            );
          })}
        </svg>

        {/* Card layer */}
        {allNodes.map((node) => {
          const role = node.agent.role as AgentRole | null | undefined;
          const roleLabel = role && ROLE_LABELS[role] ? ROLE_LABELS[role] : role || "";
          const desc = node.agent.description || "";
          const truncDesc = desc.length > 60 ? desc.slice(0, 57) + "..." : desc;

          return (
            <div
              key={node.agent.id}
              className="org-chart-card"
              style={{ left: node.x, top: node.y }}
              onClick={() => handleCardClick(node.agent.id)}
            >
              {roleLabel && (
                <div className="org-chart-card-role">{roleLabel}</div>
              )}
              <div className="org-chart-card-name">{node.agent.name}</div>
              <div className="org-chart-card-desc">{truncDesc}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
