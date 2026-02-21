import { log } from "./logger";
import type {
  Flow,
  FlowNode,
  FlowEdge,
  FlowSummary,
  FlowRun,
  NodeTypeSchema,
  SessionInfo,
  SavedPrompt,
  FlowSessionsInfo,
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

export async function getSession(flowId: string): Promise<SessionInfo> {
  return apiFetch<SessionInfo>(`/flows/${flowId}/session`);
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

export async function listInteractSessions(
  flowId: string
): Promise<FlowSessionsInfo> {
  return apiFetch<FlowSessionsInfo>(`/flows/${flowId}/interact/sessions`);
}

export async function newInteractSession(
  flowId: string
): Promise<{ session_id: string; created_at: string; warning?: string }> {
  return apiFetch(`/flows/${flowId}/interact/new`, { method: "POST" });
}

export async function deleteInteractSession(
  flowId: string,
  sessionId: string
): Promise<{ deleted: boolean; active_session: string }> {
  return apiFetch(`/flows/${flowId}/interact/sessions/${sessionId}`, {
    method: "DELETE",
  });
}

export async function resetInteract(flowId: string): Promise<void> {
  await apiFetch(`/flows/${flowId}/interact/reset`, { method: "POST" });
}

export async function stopInteract(
  flowId: string,
  sessionId?: string
): Promise<void> {
  await apiFetch(`/flows/${flowId}/interact/stop`, {
    method: "POST",
    body: sessionId ? JSON.stringify({ session_id: sessionId }) : undefined,
  });
}

// ---------------------------------------------------------------------------
// Node-level chat API
// ---------------------------------------------------------------------------

export async function getNodeSession(
  flowId: string,
  nodeId: string
): Promise<SessionInfo> {
  return apiFetch<SessionInfo>(`/flows/${flowId}/nodes/${nodeId}/session`);
}

export async function listNodeInteractSessions(
  flowId: string,
  nodeId: string
): Promise<FlowSessionsInfo> {
  return apiFetch<FlowSessionsInfo>(
    `/flows/${flowId}/nodes/${nodeId}/interact/sessions`
  );
}

export async function newNodeInteractSession(
  flowId: string,
  nodeId: string
): Promise<{ session_id: string; created_at: string; warning?: string }> {
  return apiFetch(`/flows/${flowId}/nodes/${nodeId}/interact/new`, {
    method: "POST",
  });
}

export async function deleteNodeInteractSession(
  flowId: string,
  nodeId: string,
  sessionId: string
): Promise<{ deleted: boolean; active_session: string }> {
  return apiFetch(
    `/flows/${flowId}/nodes/${nodeId}/interact/sessions/${sessionId}`,
    { method: "DELETE" }
  );
}

export async function stopNodeInteract(
  flowId: string,
  nodeId: string,
  sessionId?: string
): Promise<void> {
  await apiFetch(`/flows/${flowId}/nodes/${nodeId}/interact/stop`, {
    method: "POST",
    body: sessionId ? JSON.stringify({ session_id: sessionId }) : undefined,
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
