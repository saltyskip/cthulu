import { log } from "./logger";
import type {
  Flow,
  FlowNode,
  FlowEdge,
  FlowSummary,
  FlowRun,
  NodeTypeSchema,
} from "../types/flow";

const DEFAULT_BASE_URL = "http://localhost:8081";

function getBaseUrl(): string {
  return localStorage.getItem("cthulu_server_url") || DEFAULT_BASE_URL;
}

export function setServerUrl(url: string) {
  localStorage.setItem("cthulu_server_url", url);
  log("info", `Server URL changed to ${url}`);
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

export async function checkConnection(): Promise<boolean> {
  const url = `${getBaseUrl()}/health/`;
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
