export type NodeType = "trigger" | "source" | "executor" | "sink";

export interface Position {
  x: number;
  y: number;
}

export interface FlowNode {
  id: string;
  node_type: NodeType;
  kind: string;
  config: Record<string, unknown>;
  position: Position;
  label: string;
}

export interface FlowEdge {
  id: string;
  source: string;
  target: string;
}

export interface Flow {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  nodes: FlowNode[];
  edges: FlowEdge[];
  created_at: string;
  updated_at: string;
}

export interface FlowSummary {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  node_count: number;
  edge_count: number;
  created_at: string;
  updated_at: string;
}

export type RunStatus = "running" | "success" | "failed";

export interface NodeRun {
  node_id: string;
  status: RunStatus;
  started_at: string;
  finished_at: string | null;
  output_preview: string | null;
}

export interface FlowRun {
  id: string;
  flow_id: string;
  status: RunStatus;
  started_at: string;
  finished_at: string | null;
  node_runs: NodeRun[];
  error: string | null;
}

export interface NodeTypeSchema {
  kind: string;
  node_type: NodeType;
  label: string;
  config_schema: Record<string, unknown>;
}

export interface RunEvent {
  flow_id: string;
  run_id: string;
  timestamp: string;
  node_id: string | null;
  event_type: string;
  message: string;
}
