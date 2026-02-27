export const STUDIO_ASSISTANT_ID = "studio-assistant";

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

export interface SessionInfo {
  flow_id: string;
  flow_name: string;
  prompt: string;
  permissions: string[];
  append_system_prompt: string | null;
  working_dir: string;
  sources_summary: string;
  sinks_summary: string;
}

export interface OutputLine {
  type: "system" | "text" | "tool_use" | "tool_result" | "result" | "error" | "cost";
  text: string;
}

export interface InteractSessionInfo {
  session_id: string;
  summary: string;
  message_count: number;
  total_cost: number;
  created_at: string;
  busy: boolean;
}

export interface FlowSessionsInfo {
  flow_name: string;
  active_session: string;
  sessions: InteractSessionInfo[];
}

export interface SavedPrompt {
  id: string;
  title: string;
  summary: string;
  source_flow_name: string;
  tags: string[];
  created_at: string;
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export interface Agent {
  id: string;
  name: string;
  description: string;
  prompt: string;
  permissions: string[];
  append_system_prompt: string | null;
  working_dir: string | null;
  created_at: string;
  updated_at: string;
}

export interface AgentSummary {
  id: string;
  name: string;
  description: string;
  permissions: string[];
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// Template Gallery
// ---------------------------------------------------------------------------

/** Minimal pipeline shape — used to render the mini flow diagram on each card. */
export interface PipelineShape {
  trigger: string;
  sources: string[];
  executors: string[];
  sinks: string[];
}

/** Metadata for a single workflow template loaded from static/workflows/. */
export interface TemplateMetadata {
  slug: string;
  category: string;
  title: string;
  description: string;
  tags: string[];
  estimated_cost: string | null;
  icon: string | null;
  pipeline_shape: PipelineShape;
  raw_yaml: string;
}
