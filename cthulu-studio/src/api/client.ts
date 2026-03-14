import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  HeartbeatRun,
  EnvironmentTestResult,
  Org,
  ProjectMeta,
  Task,
} from "../types/flow";

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

export async function listFlows(): Promise<FlowSummary[]> {
  log("http", "invoke list_flows");
  const data = await invoke<{ flows: FlowSummary[] }>("list_flows");
  return data.flows;
}

export async function getFlow(id: string): Promise<Flow> {
  log("http", `invoke get_flow id=${id}`);
  return invoke<Flow>("get_flow", { id });
}

export async function createFlow(
  name: string,
  description?: string,
  nodes?: FlowNode[],
  edges?: FlowEdge[]
): Promise<{ id: string }> {
  log("http", `invoke create_flow name=${name}`);
  return invoke<{ id: string }>("create_flow", {
    name,
    description: description || "",
    nodes: nodes || [],
    edges: edges || [],
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
  log("http", `invoke update_flow id=${id}`);
  return invoke<Flow>("update_flow", { id, updates });
}

export async function deleteFlow(id: string): Promise<void> {
  log("http", `invoke delete_flow id=${id}`);
  await invoke("delete_flow", { id });
}

export async function triggerFlow(
  id: string
): Promise<{ status: string; flow_id: string }> {
  log("http", `invoke trigger_flow id=${id}`);
  return invoke<{ status: string; flow_id: string }>("trigger_flow", { id });
}

export async function getFlowRuns(id: string): Promise<FlowRun[]> {
  log("http", `invoke get_flow_runs id=${id}`);
  const data = await invoke<{ runs: FlowRun[] }>("get_flow_runs", { id });
  return data.runs;
}

export async function getNodeTypes(): Promise<NodeTypeSchema[]> {
  log("http", "invoke get_node_types");
  const data = await invoke<{ node_types: NodeTypeSchema[] }>("get_node_types");
  return data.node_types;
}

export interface PromptFile {
  path: string;
  filename: string;
  title: string;
}

export async function listPromptFiles(): Promise<PromptFile[]> {
  log("http", "invoke list_prompt_files");
  const data = await invoke<{ files: PromptFile[] }>("list_prompt_files");
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
  log("http", `invoke list_agent_sessions agentId=${agentId}`);
  return invoke<AgentSessionsInfo>("list_agent_sessions", { agentId });
}

export async function newAgentSession(
  agentId: string
): Promise<{ session_id: string; created_at: string; warning?: string }> {
  log("http", `invoke new_agent_session agentId=${agentId}`);
  return invoke<{ session_id: string; created_at: string; warning?: string }>(
    "new_agent_session",
    { agentId }
  );
}

export async function deleteAgentSession(
  agentId: string,
  sessionId: string
): Promise<{ deleted: boolean; active_session: string }> {
  log("http", `invoke delete_agent_session agentId=${agentId} sessionId=${sessionId}`);
  return invoke<{ deleted: boolean; active_session: string }>(
    "delete_agent_session",
    { agentId, sessionId }
  );
}

export async function stopAgentChat(
  agentId: string,
  sessionId?: string
): Promise<void> {
  log("http", `invoke stop_agent_chat agentId=${agentId}`);
  await invoke("stop_agent_chat", { agentId, sessionId });
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
  log("http", `invoke get_session_status agentId=${agentId} sessionId=${sessionId}`);
  return invoke<SessionStatus>("get_session_status", { agentId, sessionId });
}

export async function killSession(
  agentId: string,
  sessionId: string
): Promise<void> {
  log("http", `invoke kill_session agentId=${agentId} sessionId=${sessionId}`);
  await invoke("kill_session", { agentId, sessionId });
}

// ---------------------------------------------------------------------------
// Hooks / Permissions
// ---------------------------------------------------------------------------

/** Respond to a pending permission request (Allow or Deny). */
export async function respondToPermission(
  requestId: string,
  decision: "allow" | "deny"
): Promise<{ ok: boolean }> {
  log("http", `invoke respond_to_permission requestId=${requestId} decision=${decision}`);
  return invoke<{ ok: boolean }>("permission_response", {
    request: { request_id: requestId, decision },
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
  log("http", `invoke list_session_files agentId=${agentId} sessionId=${sessionId}`);
  return invoke<{ tree: FileTreeEntry[]; root: string }>(
    "list_session_files",
    { agentId, sessionId }
  );
}

/** Read a file from a session's working directory (read-only). */
export async function readSessionFile(
  agentId: string,
  sessionId: string,
  path: string
): Promise<{ path: string; content: string; size: number }> {
  log("http", `invoke read_session_file agentId=${agentId} path=${path}`);
  return invoke<{ path: string; content: string; size: number }>(
    "read_session_file",
    { agentId, sessionId, path }
  );
}

/** Fetch git status snapshot for a session. Returns null if no git integration. */
export async function getGitSnapshot(
  agentId: string,
  sessionId: string
): Promise<import("../components/chat/FilePreviewContext").MultiRepoSnapshot | null> {
  try {
    return await invoke<import("../components/chat/FilePreviewContext").MultiRepoSnapshot>(
      "get_git_snapshot",
      { agentId, sessionId }
    );
  } catch {
    return null; // no git integration
  }
}

/** Fetch unified diff for a single file in a git session. */
export async function getGitDiff(
  agentId: string,
  sessionId: string,
  path: string,
  repoRoot?: string
): Promise<{ diff: string; path: string; repo_root: string }> {
  log("http", `invoke get_git_diff agentId=${agentId} path=${path}`);
  return invoke<{ diff: string; path: string; repo_root: string }>(
    "get_git_diff",
    { agentId, sessionId, path, repoRoot: repoRoot && repoRoot !== "." ? repoRoot : undefined }
  );
}

/** Fetch the full JSONL log for a completed flow-run session. */
export async function getSessionLog(
  agentId: string,
  sessionId: string
): Promise<string[]> {
  log("http", `invoke get_session_log agentId=${agentId} sessionId=${sessionId}`);
  const data = await invoke<{ lines: string[] }>("get_session_log", {
    agentId,
    sessionId,
  });
  return data.lines;
}

/** Subscribe to a live flow-run session log via Tauri events. Returns cleanup function. */
export function streamSessionLog(
  agentId: string,
  sessionId: string,
  onLine: (line: string) => void,
  onDone: () => void
): () => void {
  let unlisten: UnlistenFn | null = null;
  let cleaned = false;

  listen<{ line?: string; done?: boolean }>(
    `session-log-${agentId}-${sessionId}`,
    (event) => {
      if (cleaned) return;
      if (event.payload.done) {
        onDone();
      } else if (event.payload.line != null) {
        onLine(event.payload.line);
      }
    }
  ).then((fn) => {
    unlisten = fn;
    if (cleaned) fn(); // was cleaned up before listener attached
  });

  return () => {
    cleaned = true;
    unlisten?.();
  };
}

export async function listPrompts(): Promise<SavedPrompt[]> {
  log("http", "invoke list_prompts");
  const data = await invoke<{ prompts: SavedPrompt[] }>("list_prompts");
  return data.prompts;
}

export async function getPrompt(id: string): Promise<SavedPrompt> {
  log("http", `invoke get_prompt id=${id}`);
  return invoke<SavedPrompt>("get_prompt", { id });
}

export async function savePrompt(prompt: {
  title: string;
  summary: string;
  source_flow_name: string;
  tags: string[];
}): Promise<{ id: string }> {
  log("http", "invoke save_prompt");
  return invoke<{ id: string }>("create_prompt", { request: prompt });
}

export async function updatePrompt(
  id: string,
  updates: { title?: string; summary?: string; tags?: string[] }
): Promise<SavedPrompt> {
  log("http", `invoke update_prompt id=${id}`);
  return invoke<SavedPrompt>("update_prompt", { id, request: updates });
}

export async function deletePrompt(id: string): Promise<void> {
  log("http", `invoke delete_prompt id=${id}`);
  await invoke("delete_prompt", { id });
}

export async function summarizeSession(
  transcript: string,
  flowName: string,
  flowDescription: string
): Promise<{ title: string; summary: string; tags: string[] }> {
  log("http", "invoke summarize_session");
  return invoke<{ title: string; summary: string; tags: string[] }>(
    "summarize_session",
    { request: { transcript, flow_name: flowName, flow_description: flowDescription } }
  );
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
  log("http", `invoke get_flow_schedule flowId=${flowId}`);
  return invoke<ScheduleInfo>("get_flow_schedule", { flowId });
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
  log("http", "invoke get_scheduler_status");
  return invoke<SchedulerStatus>("get_scheduler_status");
}

export interface CronValidation {
  valid: boolean;
  expression?: string;
  error?: string;
  next_runs: string[];
}

export async function validateCron(
  expression: string
): Promise<CronValidation> {
  log("http", "invoke validate_cron");
  return invoke<CronValidation>("validate_cron", { expression });
}

// ---------------------------------------------------------------------------
// Auth / Token management
// ---------------------------------------------------------------------------

export async function getTokenStatus(): Promise<{ has_token: boolean }> {
  log("http", "invoke get_token_status");
  return invoke<{ has_token: boolean }>("token_status");
}

export async function refreshToken(): Promise<{
  ok: boolean;
  message: string;
}> {
  log("http", "invoke refresh_token");
  return invoke<{ ok: boolean; message: string }>("refresh_token");
}

// ---------------------------------------------------------------------------
// Template Gallery
// ---------------------------------------------------------------------------

/** Fetch all workflow templates (all categories). */
export async function listTemplates(): Promise<TemplateMetadata[]> {
  log("http", "invoke list_templates");
  const data = await invoke<{ templates: TemplateMetadata[] }>(
    "list_templates"
  );
  return data.templates;
}

/** Fetch raw YAML for a single template. */
export async function getTemplateYaml(
  category: string,
  slug: string
): Promise<string> {
  log("http", `invoke get_template_yaml category=${category} slug=${slug}`);
  return invoke<string>("get_template_yaml", { category, slug });
}

/** Parse + save a template as a new Flow. Returns the created Flow. */
export async function importTemplate(
  category: string,
  slug: string
): Promise<Flow> {
  log("http", `invoke import_template category=${category} slug=${slug}`);
  return invoke<Flow>("import_template", { category, slug });
}

export interface ImportResult {
  flows: Flow[];
  errors: { file: string; error: string }[];
  total_found: number;
  imported: number;
}

/** Upload raw YAML text and import it as a new Flow. */
export async function importYaml(yaml: string): Promise<ImportResult> {
  log("http", "invoke import_yaml");
  return invoke<ImportResult>("import_yaml", { request: { yaml } });
}

/** Fetch all workflow YAMLs from a GitHub repo and import them. */
export async function importFromGithub(
  repoUrl: string,
  path = "",
  branch = "main"
): Promise<ImportResult> {
  log("http", `invoke import_from_github repo=${repoUrl}`);
  return invoke<ImportResult>("import_github", {
    request: { repo_url: repoUrl, path, branch },
  });
}

// ---------------------------------------------------------------------------
// Agent CRUD
// ---------------------------------------------------------------------------

export async function listAgents(): Promise<AgentSummary[]> {
  log("http", "invoke list_agents");
  const data = await invoke<{ agents: AgentSummary[] }>("list_agents");
  return data.agents;
}

export async function getAgent(id: string): Promise<Agent> {
  log("http", `invoke get_agent id=${id}`);
  return invoke<Agent>("get_agent", { id });
}

export async function createAgent(data: {
  name: string;
  description?: string;
  prompt?: string;
  permissions?: string[];
  append_system_prompt?: string | null;
  working_dir?: string | null;
}): Promise<{ id: string }> {
  log("http", `invoke create_agent name=${data.name}`);
  return invoke<{ id: string }>("create_agent", { request: data });
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
    heartbeat_enabled?: boolean;
    heartbeat_interval_secs?: number;
    heartbeat_prompt_template?: string;
    max_turns_per_heartbeat?: number;
    auto_permissions?: boolean;
    project?: string | null;
    role?: string | null;
    reports_to?: string | null;
  }
): Promise<Agent> {
  log("http", `invoke update_agent id=${id}`);
  return invoke<Agent>("update_agent", { id, request: updates });
}

export async function deleteAgent(id: string): Promise<void> {
  log("http", `invoke delete_agent id=${id}`);
  await invoke("delete_agent", { id });
}

// ---------------------------------------------------------------------------
// File Change Subscriptions (Tauri events)
// ---------------------------------------------------------------------------

export interface ResourceChangeEvent {
  resource_type: "flow" | "agent" | "prompt";
  change_type: "created" | "updated" | "deleted";
  resource_id: string;
  timestamp: string;
}

/**
 * Subscribe to real-time resource change events via Tauri event system.
 * Returns a cleanup function to stop listening.
 */
export function subscribeToChanges(
  onEvent: (event: ResourceChangeEvent) => void
): () => void {
  let unlisten: UnlistenFn | null = null;
  let cleaned = false;

  listen<ResourceChangeEvent>("resource-change", (event) => {
    if (cleaned) return;
    onEvent(event.payload);
  }).then((fn) => {
    unlisten = fn;
    if (cleaned) fn();
  });

  return () => {
    cleaned = true;
    unlisten?.();
  };
}

// ---------------------------------------------------------------------------
// Secrets / GitHub PAT
// ---------------------------------------------------------------------------

export async function getGithubPatStatus(): Promise<{ configured: boolean }> {
  log("http", "invoke get_github_pat_status");
  return invoke<{ configured: boolean }>("get_github_pat_status");
}

export async function saveGithubPat(
  token: string
): Promise<{ ok: boolean; username: string }> {
  log("http", "invoke save_github_pat");
  return invoke<{ ok: boolean; username: string }>("save_github_pat", {
    request: { token },
  });
}

// ---------------------------------------------------------------------------
// Workflows (GitHub-backed)
// ---------------------------------------------------------------------------

export async function setupWorkflows(): Promise<{
  repo_url: string;
  created: boolean;
  username: string;
}> {
  log("http", "invoke setup_workflows");
  return invoke<{ repo_url: string; created: boolean; username: string }>(
    "setup_workflows_repo"
  );
}

export async function listWorkspaces(): Promise<{ workspaces: string[] }> {
  log("http", "invoke list_workspaces");
  return invoke<{ workspaces: string[] }>("list_workspaces");
}

export async function createWorkspace(
  name: string
): Promise<{ ok: boolean; name: string }> {
  log("http", `invoke create_workspace name=${name}`);
  return invoke<{ ok: boolean; name: string }>("create_workspace", { request: { name } });
}

export async function listWorkflows(
  workspace: string
): Promise<{
  workspace: string;
  workflows: import("../types/flow").WorkflowSummary[];
}> {
  log("http", `invoke list_workflows workspace=${workspace}`);
  return invoke<{
    workspace: string;
    workflows: import("../types/flow").WorkflowSummary[];
  }>("list_workspace_workflows", { workspace });
}

export async function getWorkflow(
  workspace: string,
  name: string
): Promise<Record<string, unknown>> {
  log("http", `invoke get_workflow workspace=${workspace} name=${name}`);
  return invoke<Record<string, unknown>>("get_workflow", { workspace, name });
}

export async function saveWorkflow(
  workspace: string,
  name: string,
  flow: Record<string, unknown>
): Promise<{ ok: boolean }> {
  log("http", `invoke save_workflow workspace=${workspace} name=${name}`);
  return invoke<{ ok: boolean }>("save_workflow", { workspace, name, request: { flow } });
}

export async function publishWorkflow(
  workspace: string,
  name: string,
  flow: Record<string, unknown>
): Promise<{ ok: boolean }> {
  log("http", `invoke publish_workflow workspace=${workspace} name=${name}`);
  return invoke<{ ok: boolean }>("publish_workflow", {
    workspace,
    name,
    request: { flow },
  });
}

export async function deleteWorkflow(
  workspace: string,
  name: string
): Promise<{ ok: boolean }> {
  log("http", `invoke delete_workflow workspace=${workspace} name=${name}`);
  return invoke<{ ok: boolean }>("delete_workflow", { workspace, name });
}

export async function syncWorkflows(): Promise<{
  ok: boolean;
  workspaces: string[];
}> {
  log("http", "invoke sync_workflows");
  return invoke<{ ok: boolean; workspaces: string[] }>("sync_workflows");
}

export async function runWorkflow(
  workspace: string,
  name: string,
): Promise<{ status: string; workspace: string; name: string }> {
  log("info", `invoke run_workflow workspace=${workspace} name=${name}`);
  return invoke<{ status: string; workspace: string; name: string }>(
    "run_workflow",
    { workspace, name },
  );
}

// ---------------------------------------------------------------------------
// Claude CLI Status
// ---------------------------------------------------------------------------

export async function getClaudeStatus(): Promise<EnvironmentTestResult> {
  log("http", "invoke get_claude_status");
  return invoke<EnvironmentTestResult>("claude_status");
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

export async function wakeupAgent(agentId: string): Promise<HeartbeatRun> {
  log("http", `invoke wakeup_agent agentId=${agentId}`);
  return invoke<HeartbeatRun>("wakeup_agent", { agentId });
}

export async function listHeartbeatRuns(
  agentId: string
): Promise<HeartbeatRun[]> {
  log("http", `invoke list_heartbeat_runs agentId=${agentId}`);
  return invoke<HeartbeatRun[]>("list_heartbeat_runs", { agentId });
}

export async function getHeartbeatRun(
  agentId: string,
  runId: string
): Promise<HeartbeatRun> {
  log("http", `invoke get_heartbeat_run agentId=${agentId} runId=${runId}`);
  return invoke<HeartbeatRun>("get_heartbeat_run", { agentId, runId });
}

export async function getHeartbeatRunLog(
  agentId: string,
  runId: string
): Promise<{ lines: string[] }> {
  log("http", `invoke get_heartbeat_run_log agentId=${agentId} runId=${runId}`);
  return invoke<{ lines: string[] }>("get_heartbeat_run_log", {
    agentId,
    runId,
  });
}

// ---------------------------------------------------------------------------
// Setup / Configuration
// ---------------------------------------------------------------------------

export async function checkSetupStatus(): Promise<{
  setup_complete: boolean;
  github_pat_configured: boolean;
  claude_oauth_configured: boolean;
  anthropic_api_key_configured: boolean;
  openai_api_key_configured: boolean;
  slack_webhook_configured: boolean;
  notion_configured: boolean;
  telegram_configured: boolean;
}> {
  return invoke("check_setup_status");
}

export async function saveAnthropicKey(key: string): Promise<{ ok: boolean }> {
  return invoke("save_anthropic_key", { request: { key } });
}

export async function saveOpenaiKey(key: string): Promise<{ ok: boolean }> {
  return invoke("save_openai_key", { request: { key } });
}

export async function saveSlackWebhook(url: string): Promise<{ ok: boolean }> {
  return invoke("save_slack_webhook", { request: { url } });
}

export async function saveNotionCredentials(
  token: string,
  databaseId: string
): Promise<{ ok: boolean }> {
  return invoke("save_notion_credentials", {
    request: { token, database_id: databaseId },
  });
}

export async function saveTelegramCredentials(
  botToken: string,
  chatId: string
): Promise<{ ok: boolean }> {
  return invoke("save_telegram_credentials", {
    request: { bot_token: botToken, chat_id: chatId },
  });
}

// ---------------------------------------------------------------------------
// PTY (Terminal)
// ---------------------------------------------------------------------------

export async function spawnPty(
  agentId: string,
  sessionId: string,
  opts?: { workingDirOverride?: string; workspace?: string; workflowName?: string },
): Promise<{ session_id: string }> {
  log("info", `invoke spawn_pty agentId=${agentId} sessionId=${sessionId}${opts?.workspace ? ` ws=${opts.workspace}` : ""}${opts?.workflowName ? ` wf=${opts.workflowName}` : ""}${opts?.workingDirOverride ? ` dir=${opts.workingDirOverride}` : ""}`);
  return invoke<{ session_id: string }>("spawn_pty", {
    agentId,
    sessionId,
    workingDirOverride: opts?.workingDirOverride ?? null,
    workspace: opts?.workspace ?? null,
    workflowName: opts?.workflowName ?? null,
  });
}

export async function writePty(
  sessionId: string,
  data: string,
): Promise<void> {
  await invoke("write_pty", { sessionId, data });
}

export async function resizePty(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  await invoke("resize_pty", {
    sessionId,
    cols: Math.floor(cols),
    rows: Math.floor(rows),
  });
}

export async function killPty(sessionId: string): Promise<void> {
  log("info", `invoke kill_pty sessionId=${sessionId}`);
  await invoke("kill_pty", { sessionId });
}

// ---------------------------------------------------------------------------
// Agent Repo Sync
// ---------------------------------------------------------------------------

export async function setupAgentRepo(): Promise<{
  repo_url: string;
  created: boolean;
  username: string;
}> {
  log("http", "invoke setup_agent_repo");
  return invoke<{ repo_url: string; created: boolean; username: string }>(
    "setup_agent_repo"
  );
}

export async function listAgentProjects(org: string): Promise<ProjectMeta[]> {
  log("http", `invoke list_agent_projects org=${org}`);
  const data = await invoke<{ projects: ProjectMeta[] }>("list_agent_projects", { org });
  return data.projects;
}

export async function createAgentProject(
  org: string,
  project: string,
  workingDir?: string
): Promise<{ ok: boolean; project: string }> {
  log("http", `invoke create_agent_project org=${org} project=${project}`);
  return invoke<{ ok: boolean; project: string }>("create_agent_project", {
    org,
    project,
    working_dir: workingDir || null,
  });
}

export async function publishAgent(
  id: string,
  org: string,
  project: string
): Promise<{ ok: boolean; id: string; project: string }> {
  log("http", `invoke publish_agent id=${id} org=${org} project=${project}`);
  return invoke<{ ok: boolean; id: string; project: string }>(
    "publish_agent",
    { id, org, project }
  );
}

export async function unpublishAgent(
  id: string,
  org: string
): Promise<{ ok: boolean; id: string }> {
  log("http", `invoke unpublish_agent id=${id} org=${org}`);
  return invoke<{ ok: boolean; id: string }>("unpublish_agent", { id, org });
}

export async function syncAgentRepo(): Promise<{
  ok: boolean;
  synced: number;
}> {
  log("http", "invoke sync_agent_repo");
  return invoke<{ ok: boolean; synced: number }>("sync_agent_repo");
}

// ---------------------------------------------------------------------------
// Org management
// ---------------------------------------------------------------------------

export async function listOrgs(): Promise<Org[]> {
  log("http", "invoke list_orgs");
  const data = await invoke<{ orgs: Org[] }>("list_orgs");
  return data.orgs;
}

export async function createOrg(
  name: string,
  description?: string
): Promise<{ ok: boolean; slug: string; name: string }> {
  log("http", `invoke create_org name=${name}`);
  return invoke<{ ok: boolean; slug: string; name: string }>("create_org", {
    name,
    description: description ?? null,
  });
}

export async function deleteOrg(slug: string): Promise<{ ok: boolean }> {
  log("http", `invoke delete_org slug=${slug}`);
  return invoke<{ ok: boolean }>("delete_org", { slug });
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

export async function listTasks(assignee?: string): Promise<Task[]> {
  log("http", `invoke list_tasks assignee=${assignee ?? "all"}`);
  const data = await invoke<{ tasks: Task[] }>("list_tasks", { assignee: assignee ?? null });
  return data.tasks;
}

export async function createTask(title: string, assigneeAgentId: string): Promise<Task> {
  log("http", `invoke create_task title=${title} assignee=${assigneeAgentId}`);
  return invoke<Task>("create_task", {
    request: { title, assignee_agent_id: assigneeAgentId },
  });
}

export async function updateTask(
  id: string,
  updates: { title?: string; status?: string; assignee_agent_id?: string }
): Promise<Task> {
  log("http", `invoke update_task id=${id}`);
  return invoke<Task>("update_task", { id, request: updates });
}

export async function deleteTask(id: string): Promise<void> {
  log("http", `invoke delete_task id=${id}`);
  await invoke("delete_task", { id });
}
