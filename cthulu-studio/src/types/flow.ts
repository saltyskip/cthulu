export const STUDIO_ASSISTANT_ID = "studio-assistant";

export type NodeType = "trigger" | "source" | "executor" | "sink";

export type ActiveView = "flow-editor" | "agent-workspace" | "agent-list" | "agent-detail" | "prompt-editor" | "workflows" | "org-chart";

export interface Org {
  slug: string;
  name: string;
  description: string;
}

export interface ProjectMeta {
  slug: string;
  name: string;
  description: string;
  working_dir: string;
  color: string | null;
  status: "active" | "archived";
}

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
  version: number;
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
  project?: string | null;
  reports_to?: string | null;
  role?: string | null;
  heartbeat_enabled: boolean;
  heartbeat_interval_secs: number;
  heartbeat_prompt_template: string;
  max_turns_per_heartbeat: number;
  auto_permissions: boolean;
  created_at: string;
  updated_at: string;
}

export interface AgentSummary {
  id: string;
  name: string;
  description: string;
  permissions: string[];
  subagent_only?: boolean;
  subagent_count?: number;
  reports_to?: string | null;
  role?: string | null;
  created_at: string;
  updated_at: string;
  project?: string | null;
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

// ---------------------------------------------------------------------------
// Workflows (GitHub-backed)
// ---------------------------------------------------------------------------

export interface WorkflowSummary {
  name: string;
  workspace: string;
  description?: string;
  node_count: number;
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

export type HeartbeatRunStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'timed_out' | 'cancelled';

export type WakeupSource = 'timer' | 'on_demand' | 'assignment';

export interface HeartbeatRun {
  id: string;
  agent_id: string;
  status: HeartbeatRunStatus;
  source: WakeupSource;
  started_at: string;
  finished_at: string | null;
  cost_usd: number;
  usage: { input_tokens: number; cached_input_tokens: number; output_tokens: number } | null;
  error: string | null;
  log_path: string;
  model: string | null;
  session_id: string | null;
  duration_secs: number;
}

// ---------------------------------------------------------------------------
// Agent Hierarchy
// ---------------------------------------------------------------------------

export const AGENT_ROLES = [
  "ceo", "cto", "cmo", "cfo", "engineer", "designer",
  "pm", "qa", "devops", "researcher", "general",
] as const;

export type AgentRole = typeof AGENT_ROLES[number];

export const ROLE_LABELS: Record<AgentRole, string> = {
  ceo: "CEO",
  cto: "CTO",
  cmo: "CMO",
  cfo: "CFO",
  engineer: "Engineer",
  designer: "Designer",
  pm: "Product Manager",
  qa: "QA",
  devops: "DevOps",
  researcher: "Researcher",
  general: "General",
};

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

export type TaskStatus = 'todo' | 'in_progress' | 'done' | 'cancelled';

export interface Task {
  id: string;
  title: string;
  status: TaskStatus;
  assignee_agent_id: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// Environment / Claude CLI Status
// ---------------------------------------------------------------------------

export type CheckLevel = 'info' | 'warn' | 'error';
export type CheckStatus = 'pass' | 'warn' | 'fail';

export interface EnvironmentCheck {
  code: string;
  level: CheckLevel;
  message: string;
  hint?: string;
}

export interface EnvironmentTestResult {
  status: CheckStatus;
  checks: EnvironmentCheck[];
  tested_at: string;
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
