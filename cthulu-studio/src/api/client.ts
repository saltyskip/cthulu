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

// ── Auth token injection ─────────────────────────────────────

let getTokenFn: (() => Promise<string | null>) | null = null;

/** Called by AuthGate to wire token getter into all API requests. */
export function setAuthTokenGetter(fn: (() => Promise<string | null>) | null) {
  getTokenFn = fn;
}

/** Get the current auth token (if available). Used by SSE EventSource callers. */
export async function getAuthToken(): Promise<string | null> {
  return getTokenFn ? getTokenFn() : null;
}

/** Get auth token synchronously from localStorage (for EventSource URLs). */
export function getAuthTokenSync(): string | null {
  return localStorage.getItem("cthulu_auth_token");
}

/** Append ?token= to a URL for EventSource connections (which can't set headers). */
export function withAuthToken(url: string): string {
  const token = getAuthTokenSync();
  if (!token) return url;
  const sep = url.includes("?") ? "&" : "?";
  return `${url}${sep}token=${encodeURIComponent(token)}`;
}

/** Get auth headers for fetch-based streaming (non-apiFetch callers). */
export async function getAuthHeaders(): Promise<Record<string, string>> {
  const token = await getAuthToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function apiFetch<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${getBaseUrl()}/api${path}`;
  const method = options.method || "GET";

  log("http", `${method} ${path}`);
  const start = performance.now();

  // Attach auth token if available
  const authHeaders: Record<string, string> = {};
  if (getTokenFn) {
    const token = await getTokenFn();
    if (token) {
      authHeaders["Authorization"] = `Bearer ${token}`;
    }
  }

  try {
    const res = await fetch(url, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...authHeaders,
        ...options.headers,
      },
    });

    const elapsed = Math.round(performance.now() - start);

    if (!res.ok) {
      // Auto-logout on 401 (expired/invalid token)
      if (res.status === 401 && getTokenFn) {
        localStorage.removeItem("cthulu_auth_token");
        window.location.reload();
        return new Promise(() => {}) as T; // prevent downstream error handling during reload
      }
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
    version?: number;
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
  interactive_count?: number;
  max_interactive_sessions?: number;
}

export interface FlowRunMeta {
  flow_id: string;
  flow_name: string;
  run_id: string;
  node_id: string;
  node_label: string;
}

export interface InteractSessionInfo {
  session_id: string;
  summary: string;
  message_count: number;
  total_cost: number;
  created_at: string;
  busy: boolean;
  process_alive?: boolean;
  kind: "interactive" | "flow_run";
  flow_run?: FlowRunMeta;
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

export interface SessionStatus {
  session_id: string;
  busy: boolean;
  busy_since: string | null;
  process_alive: boolean;
  message_count: number;
  total_cost: number;
}

export async function getSessionStatus(
  agentId: string,
  sessionId: string
): Promise<SessionStatus> {
  return apiFetch<SessionStatus>(
    `/agents/${agentId}/sessions/${sessionId}/status`
  );
}

export async function killSession(
  agentId: string,
  sessionId: string
): Promise<void> {
  await apiFetch(`/agents/${agentId}/sessions/${sessionId}/kill`, {
    method: "POST",
  });
}

// ---------------------------------------------------------------------------
// Hooks / Permissions
// ---------------------------------------------------------------------------

/** Respond to a pending permission request (Allow or Deny). */
export async function respondToPermission(
  requestId: string,
  decision: "allow" | "deny"
): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/hooks/permission-response", {
    method: "POST",
    body: JSON.stringify({ request_id: requestId, decision }),
  });
}

// ---------------------------------------------------------------------------
// File Explorer
// ---------------------------------------------------------------------------

export interface FileTreeEntry {
  name: string;
  path: string;
  type: "file" | "directory";
  size?: number;
  children?: FileTreeEntry[];
}

/** List files in a session's working directory. */
export async function listSessionFiles(
  agentId: string,
  sessionId: string
): Promise<{ tree: FileTreeEntry[]; root: string }> {
  return apiFetch(`/agents/${agentId}/sessions/${sessionId}/files`);
}

/** Read a file from a session's working directory (read-only). */
export async function readSessionFile(
  agentId: string,
  sessionId: string,
  path: string
): Promise<{ path: string; content: string; size: number }> {
  return apiFetch(`/agents/${agentId}/sessions/${sessionId}/files/read?path=${encodeURIComponent(path)}`);
}

/** Fetch git status snapshot for a session. Returns null if no git integration. */
export async function getGitSnapshot(
  agentId: string,
  sessionId: string
): Promise<import("../components/chat/FilePreviewContext").MultiRepoSnapshot | null> {
  try {
    return await apiFetch(`/agents/${agentId}/sessions/${sessionId}/git`);
  } catch {
    return null; // 404 = no git integration
  }
}

/** Fetch unified diff for a single file in a git session. */
export async function getGitDiff(
  agentId: string,
  sessionId: string,
  path: string,
  repoRoot?: string
): Promise<{ diff: string; path: string; repo_root: string }> {
  const params = new URLSearchParams({ path });
  if (repoRoot && repoRoot !== ".") params.set("repo_root", repoRoot);
  return apiFetch(`/agents/${agentId}/sessions/${sessionId}/git/diff?${params}`);
}

/** Fetch the full JSONL log for a completed flow-run session. */
export async function getSessionLog(
  agentId: string,
  sessionId: string
): Promise<string[]> {
  const data = await apiFetch<{ lines: string[] }>(
    `/agents/${agentId}/sessions/${sessionId}/log`
  );
  return data.lines;
}

/** Subscribe to a live flow-run session via SSE. Returns cleanup function. */
export function streamSessionLog(
  agentId: string,
  sessionId: string,
  onLine: (line: string) => void,
  onDone: () => void
): () => void {
  const url = withAuthToken(`${getBaseUrl()}/api/agents/${agentId}/sessions/${sessionId}/stream`);
  const source = new EventSource(url);

  source.addEventListener("line", (e: MessageEvent) => {
    onLine(e.data);
  });

  source.addEventListener("done", () => {
    onDone();
    source.close();
  });

  source.onerror = () => {
    log("warn", "Session stream SSE error — closing");
    onDone();
    source.close();
  };

  return () => source.close();
}

export async function listPrompts(): Promise<SavedPrompt[]> {
  const data = await apiFetch<{ prompts: SavedPrompt[] }>("/prompts");
  return data.prompts;
}

export async function getPrompt(id: string): Promise<SavedPrompt> {
  return apiFetch<SavedPrompt>(`/prompts/${id}`);
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
  team_id?: string | null;
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
  const url = withAuthToken(`${getBaseUrl()}/api/changes`);
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

// ── Dashboard ──────────────────────────────────────────

export interface DashboardConfig {
  channels: string[];
  slack_token_env: string;
  first_run: boolean;
}

export interface SlackMessage {
  time: string;
  user: string;
  text: string;
  ts: string;
  thread_ts?: string;
  reply_count?: number;
  replies?: SlackMessage[];
}

export interface SlackChannelMessages {
  channel: string;
  count: number;
  messages: SlackMessage[];
}

export interface DashboardMessages {
  channels: SlackChannelMessages[];
  fetched_at: string;
}

export interface ChannelSummary {
  channel: string;
  summary: string;
}

export interface DashboardSummaryResponse {
  summaries: ChannelSummary[];
  generated_at: string;
  raw?: boolean;
}

export async function getDashboardConfig(): Promise<DashboardConfig> {
  return apiFetch<DashboardConfig>("/dashboard/config");
}

export async function saveDashboardConfig(
  channels: string[],
  slackTokenEnv?: string
): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/dashboard/config", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      channels,
      slack_token_env: slackTokenEnv || "SLACK_USER_TOKEN",
    }),
  });
}

export async function getDashboardMessages(): Promise<DashboardMessages> {
  return apiFetch<DashboardMessages>("/dashboard/messages");
}

export async function getDashboardSummary(
  channels: SlackChannelMessages[]
): Promise<DashboardSummaryResponse> {
  return apiFetch<DashboardSummaryResponse>("/dashboard/summary", {
    method: "POST",
    body: JSON.stringify({ channels }),
  });
}

// ── Profile ──────────────────────────────────────────────────

export interface UserProfile {
  id: string;
  email: string;
  name: string | null;
  avatar_url: string | null;
  created_at: string;
}

export async function getProfile(): Promise<UserProfile> {
  return apiFetch<UserProfile>("/auth/me");
}

export async function updateProfile(data: { name?: string; avatar_url?: string }): Promise<UserProfile> {
  return apiFetch<UserProfile>("/auth/me", { method: "PUT", body: JSON.stringify(data) });
}

export interface UserSearchResult {
  id: string;
  email: string;
  name: string | null;
}

export async function searchUsers(query: string): Promise<{ users: UserSearchResult[] }> {
  return apiFetch<{ users: UserSearchResult[] }>(`/auth/users/search?q=${encodeURIComponent(query)}`);
}

// ── Teams ────────────────────────────────────────────────────

export interface TeamMember {
  id: string;
  email: string;
  name: string | null;
}

export interface Team {
  id: string;
  name: string;
  created_by: string;
  members: TeamMember[] | string[];
  created_at: string;
}

export async function listTeams(): Promise<{ teams: Team[] }> {
  return apiFetch<{ teams: Team[] }>("/teams");
}

export async function createTeam(name: string): Promise<{ team: Team }> {
  return apiFetch<{ team: Team }>("/teams", { method: "POST", body: JSON.stringify({ name }) });
}

export async function getTeam(id: string): Promise<{ team: Team }> {
  return apiFetch<{ team: Team }>(`/teams/${id}`);
}

export async function addTeamMember(teamId: string, email: string): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>(`/teams/${teamId}/members`, { method: "POST", body: JSON.stringify({ email }) });
}

export async function removeTeamMember(teamId: string, userId: string): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>(`/teams/${teamId}/members/${userId}`, { method: "DELETE" });
}
