import { log } from "./logger";
import type {
  Flow,
  FlowNode,
  FlowEdge,
  FlowSummary,
  FlowRun,
  NodeTypeSchema,
  SavedPrompt,
  TemplateMetadata,
  Agent,
  AgentSummary,
} from "../types/flow";

const DEFAULT_BASE_URL = "http://localhost:8081";

function ensureProtocol(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, "");
  if (!trimmed.startsWith("http://") && !trimmed.startsWith("https://")) {
    return `http://${trimmed}`;
  }
  return trimmed;
}

function getBaseUrl(): string {
  const stored = localStorage.getItem("cthulu_server_url");
  return stored ? ensureProtocol(stored) : DEFAULT_BASE_URL;
}

export function setServerUrl(url: string) {
  const normalized = ensureProtocol(url);
  localStorage.setItem("cthulu_server_url", normalized);
  log("info", `Server URL changed to ${normalized}`);
}

export function getServerUrl(): string {
  return getBaseUrl();
}

export function getTerminalWsUrl(agentId: string): string {
  const wsBase = getBaseUrl().replace(/^http/, "ws");
  return `${wsBase}/api/agents/${agentId}/terminal`;
}

async function apiFetch<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${getBaseUrl()}/api${path}`;
  const method = options.method || "GET";

  log("http", `${method} ${path}`);
  const start = performance.now();

  try {
    const res = await fetch(url, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    });

    const elapsed = Math.round(performance.now() - start);

    if (!res.ok) {
      const body = await res.text();
      log("error", `${method} ${path} -> ${res.status} (${elapsed}ms)`, body);
      throw new Error(`API error ${res.status}: ${body}`);
    }

    const data = await res.json();
    log("http", `${method} ${path} -> ${res.status} (${elapsed}ms)`);
    return data;
  } catch (e) {
    if (e instanceof TypeError) {
      // Network error (server unreachable)
      log("error", `${method} ${path} -> network error`, (e as Error).message);
    }
    throw e;
  }
}

export async function listFlows(): Promise<FlowSummary[]> {
  const data = await apiFetch<{ flows: FlowSummary[] }>("/flows");
  return data.flows;
}

export async function getFlow(id: string): Promise<Flow> {
  return apiFetch<Flow>(`/flows/${id}`);
}

export async function createFlow(
  name: string,
  description?: string,
  nodes?: FlowNode[],
  edges?: FlowEdge[]
): Promise<{ id: string }> {
  return apiFetch<{ id: string }>("/flows", {
    method: "POST",
    body: JSON.stringify({
      name,
      description: description || "",
      nodes: nodes || [],
      edges: edges || [],
    }),
  });
}

export async function updateFlow(
  id: string,
  updates: {
    name?: string;
    description?: string;
    enabled?: boolean;
    nodes?: FlowNode[];
    edges?: FlowEdge[];
  }
): Promise<Flow> {
  return apiFetch<Flow>(`/flows/${id}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export async function deleteFlow(id: string): Promise<void> {
  await apiFetch(`/flows/${id}`, { method: "DELETE" });
}

export async function triggerFlow(
  id: string
): Promise<{ status: string; flow_id: string }> {
  return apiFetch(`/flows/${id}/trigger`, { method: "POST" });
}

export async function getFlowRuns(id: string): Promise<FlowRun[]> {
  const data = await apiFetch<{ runs: FlowRun[] }>(`/flows/${id}/runs`);
  return data.runs;
}

export async function getNodeTypes(): Promise<NodeTypeSchema[]> {
  const data = await apiFetch<{ node_types: NodeTypeSchema[] }>("/node-types");
  return data.node_types;
}

export interface PromptFile {
  path: string;
  filename: string;
  title: string;
}

export async function listPromptFiles(): Promise<PromptFile[]> {
  const data = await apiFetch<{ files: PromptFile[] }>("/prompt-files");
  return data.files;
}

// ---------------------------------------------------------------------------
// Agent Chat / Sessions API
// ---------------------------------------------------------------------------

export interface AgentSessionsInfo {
  agent_id: string;
  active_session: string;
  sessions: InteractSessionInfo[];
}

interface InteractSessionInfo {
  session_id: string;
  summary: string;
  message_count: number;
  total_cost: number;
  created_at: string;
  busy: boolean;
}

export async function listAgentSessions(
  agentId: string
): Promise<AgentSessionsInfo> {
  return apiFetch<AgentSessionsInfo>(`/agents/${agentId}/sessions`);
}

export async function newAgentSession(
  agentId: string
): Promise<{ session_id: string; created_at: string; warning?: string }> {
  return apiFetch(`/agents/${agentId}/sessions`, { method: "POST" });
}

export async function deleteAgentSession(
  agentId: string,
  sessionId: string
): Promise<{ deleted: boolean; active_session: string }> {
  return apiFetch(`/agents/${agentId}/sessions/${sessionId}`, {
    method: "DELETE",
  });
}

export async function stopAgentChat(
  agentId: string,
  sessionId?: string
): Promise<void> {
  await apiFetch(`/agents/${agentId}/chat/stop`, {
    method: "POST",
    body: sessionId ? JSON.stringify({ session_id: sessionId }) : undefined,
  });
}

export async function listPrompts(): Promise<SavedPrompt[]> {
  const data = await apiFetch<{ prompts: SavedPrompt[] }>("/prompts");
  return data.prompts;
}

export async function savePrompt(prompt: {
  title: string;
  summary: string;
  source_flow_name: string;
  tags: string[];
}): Promise<{ id: string }> {
  return apiFetch<{ id: string }>("/prompts", {
    method: "POST",
    body: JSON.stringify(prompt),
  });
}

export async function updatePrompt(
  id: string,
  updates: { title?: string; summary?: string; tags?: string[] }
): Promise<SavedPrompt> {
  return apiFetch<SavedPrompt>(`/prompts/${id}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export async function deletePrompt(id: string): Promise<void> {
  await apiFetch(`/prompts/${id}`, { method: "DELETE" });
}

export async function summarizeSession(
  transcript: string,
  flowName: string,
  flowDescription: string
): Promise<{ title: string; summary: string; tags: string[] }> {
  return apiFetch("/prompts/summarize", {
    method: "POST",
    body: JSON.stringify({
      transcript,
      flow_name: flowName,
      flow_description: flowDescription,
    }),
  });
}


// ---------------------------------------------------------------------------
// Scheduler / Cron API
// ---------------------------------------------------------------------------

export interface ScheduleInfo {
  flow_id: string;
  trigger_kind: string | null;
  enabled?: boolean;
  schedule?: string;
  next_run: string | null;
  next_runs?: string[];
  poll_interval_secs?: number;
  error?: string;
}

export async function getFlowSchedule(flowId: string): Promise<ScheduleInfo> {
  return apiFetch<ScheduleInfo>(`/flows/${flowId}/schedule`);
}

export interface SchedulerFlowStatus {
  flow_id: string;
  name: string;
  enabled: boolean;
  scheduler_active: boolean;
}

export interface SchedulerStatus {
  active_count: number;
  total_flows: number;
  flows: SchedulerFlowStatus[];
}

export async function getSchedulerStatus(): Promise<SchedulerStatus> {
  return apiFetch<SchedulerStatus>("/scheduler/status");
}

export interface CronValidation {
  valid: boolean;
  expression?: string;
  error?: string;
  next_runs: string[];
}

export async function validateCron(expression: string): Promise<CronValidation> {
  return apiFetch<CronValidation>("/validate/cron", {
    method: "POST",
    body: JSON.stringify({ expression }),
  });
}

// ---------------------------------------------------------------------------
// VM Manager (Sandbox) API
// ---------------------------------------------------------------------------

export interface VmInfo {
  vm_id: number;
  tier: string;
  guest_ip: string;
  ssh_port: number;
  web_port: number;
  ssh_command: string;
  web_terminal: string;
  pid: number;
}

/** Get VM info for an executor node (returns null if no VM exists). */
export async function getNodeVm(
  flowId: string,
  nodeId: string
): Promise<VmInfo | null> {
  try {
    return await apiFetch<VmInfo>(`/sandbox/vm/${flowId}/${nodeId}`);
  } catch {
    return null;
  }
}

/** Create (or get existing) VM for an executor node. */
export async function createNodeVm(
  flowId: string,
  nodeId: string,
  tier?: string,
  apiKey?: string
): Promise<VmInfo> {
  return apiFetch<VmInfo>(`/sandbox/vm/${flowId}/${nodeId}`, {
    method: "POST",
    body: JSON.stringify({
      tier: tier || undefined,
      api_key: apiKey || undefined,
    }),
  });
}

/** Destroy the VM for an executor node. */
export async function deleteNodeVm(
  flowId: string,
  nodeId: string
): Promise<void> {
  await apiFetch(`/sandbox/vm/${flowId}/${nodeId}`, { method: "DELETE" });
}

// ---------------------------------------------------------------------------
// Auth / Token management
// ---------------------------------------------------------------------------

export async function getTokenStatus(): Promise<{ has_token: boolean }> {
  return apiFetch<{ has_token: boolean }>("/auth/token-status");
}

export async function refreshToken(): Promise<{ ok: boolean; message: string }> {
  return apiFetch<{ ok: boolean; message: string }>("/auth/refresh-token", {
    method: "POST",
  });
}

// ---------------------------------------------------------------------------
// Template Gallery
// ---------------------------------------------------------------------------

/** Fetch all workflow templates (all categories). */
export async function listTemplates(): Promise<TemplateMetadata[]> {
  const data = await apiFetch<{ templates: TemplateMetadata[] }>("/templates");
  return data.templates;
}

/** Fetch raw YAML for a single template. */
export async function getTemplateYaml(
  category: string,
  slug: string
): Promise<string> {
  const url = `${getBaseUrl()}/api/templates/${category}/${slug}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`API error ${res.status}`);
  return res.text();
}

/** Parse + save a template as a new Flow. Returns the created Flow. */
export async function importTemplate(
  category: string,
  slug: string
): Promise<Flow> {
  return apiFetch<Flow>(`/templates/${category}/${slug}/import`, {
    method: "POST",
  });
}

export interface ImportResult {
  flows: Flow[];
  errors: { file: string; error: string }[];
  total_found: number;
  imported: number;
}

/** Upload raw YAML text and import it as a new Flow. */
export async function importYaml(yaml: string): Promise<ImportResult> {
  return apiFetch<ImportResult>("/templates/import-yaml", {
    method: "POST",
    body: JSON.stringify({ yaml }),
  });
}

/** Fetch all workflow YAMLs from a GitHub repo and import them. */
export async function importFromGithub(
  repoUrl: string,
  path = "",
  branch = "main"
): Promise<ImportResult> {
  return apiFetch<ImportResult>("/templates/import-github", {
    method: "POST",
    body: JSON.stringify({ repo_url: repoUrl, path, branch }),
  });
}

// ---------------------------------------------------------------------------
// Agent CRUD
// ---------------------------------------------------------------------------

export async function listAgents(): Promise<AgentSummary[]> {
  const data = await apiFetch<{ agents: AgentSummary[] }>("/agents");
  return data.agents;
}

export async function getAgent(id: string): Promise<Agent> {
  return apiFetch<Agent>(`/agents/${id}`);
}

export async function createAgent(data: {
  name: string;
  description?: string;
  prompt?: string;
  permissions?: string[];
  append_system_prompt?: string | null;
  working_dir?: string | null;
}): Promise<{ id: string }> {
  return apiFetch<{ id: string }>("/agents", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updateAgent(
  id: string,
  updates: {
    name?: string;
    description?: string;
    prompt?: string;
    permissions?: string[];
    append_system_prompt?: string | null;
    working_dir?: string | null;
  }
): Promise<Agent> {
  return apiFetch<Agent>(`/agents/${id}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export async function deleteAgent(id: string): Promise<void> {
  await apiFetch(`/agents/${id}`, { method: "DELETE" });
}

// ---------------------------------------------------------------------------
// File Change Subscriptions (SSE)
// ---------------------------------------------------------------------------

export interface ResourceChangeEvent {
  resource_type: "flow" | "agent" | "prompt";
  change_type: "created" | "updated" | "deleted";
  resource_id: string;
  timestamp: string;
}

/**
 * Subscribe to real-time resource change events via SSE.
 * Returns a cleanup function to close the connection.
 */
export function subscribeToChanges(
  onEvent: (event: ResourceChangeEvent) => void
): () => void {
  const url = `${getBaseUrl()}/api/changes`;
  const source = new EventSource(url);

  const handler = (e: MessageEvent) => {
    try {
      const event: ResourceChangeEvent = JSON.parse(e.data);
      onEvent(event);
    } catch {
      log("warn", "Failed to parse change event", e.data);
    }
  };

  source.addEventListener("flow_change", handler);
  source.addEventListener("agent_change", handler);
  source.addEventListener("prompt_change", handler);

  source.onerror = () => {
    log("warn", "Changes SSE connection error — will auto-reconnect");
  };

  return () => source.close();
}

// ---------------------------------------------------------------------------
// Health / Connection
// ---------------------------------------------------------------------------

export async function checkConnection(): Promise<boolean> {
  const url = `${getBaseUrl()}/health`;
  const start = performance.now();

  try {
    const res = await fetch(url);
    const elapsed = Math.round(performance.now() - start);
    if (res.ok) {
      log("info", `Health check OK (${elapsed}ms)`);
      return true;
    }
    log("warn", `Health check failed: ${res.status} (${elapsed}ms)`);
    return false;
  } catch (e) {
    const elapsed = Math.round(performance.now() - start);
    log(
      "warn",
      `Health check failed (${elapsed}ms)`,
      `Cannot reach ${url} — ${(e as Error).message}`
    );
    return false;
  }
}
