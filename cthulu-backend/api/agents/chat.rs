use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use chrono::Utc;
use futures::stream::Stream;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::pin::Pin;
use uuid::Uuid;

use crate::agent_sdk::config::SessionConfig;
use crate::api::AppState;
use crate::api::FlowSessions;
use crate::api::InteractSession;
use crate::flows::{Edge, Flow, Node, NodeType};
use tokio::sync::broadcast;

/// Boxed SSE stream type — used when multiple code paths (SDK vs legacy) can
/// produce different concrete stream types.
type BoxSseStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a string to ~80 chars for use as a summary, breaking at word boundary.
fn make_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(80).collect();
    let boundary = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}...", &truncated[..boundary])
}

/// Best-effort process termination, platform-specific.
fn kill_pid(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .spawn();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .spawn();
    }
}

/// Build the path for node attachment files.
fn attachments_path(data_dir: &std::path::Path, flow_id: &str, node_id: &str) -> std::path::PathBuf {
    data_dir.join("attachments").join(flow_id).join(node_id)
}

/// The session key for an agent — just `"agent::{id}"`.
fn agent_key(agent_id: &str) -> String {
    format!("agent::{agent_id}")
}

/// Process pool key — unique per agent + session pair.
fn process_key(agent_id: &str, session_id: &str) -> String {
    format!("agent::{agent_id}::session::{session_id}")
}

/// Maximum number of interactive sessions per agent.
const MAX_INTERACTIVE_SESSIONS: usize = 5;

/// Duration after which a busy session with no live process is considered stale.
const STALE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Agent chat endpoints
// ---------------------------------------------------------------------------

/// GET /agents/{id}/sessions — list all sessions for an agent
pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let key = agent_key(&id);
    let sessions = state.interact_sessions.read().await;
    if let Some(flow_sessions) = sessions.get(&key) {
        let sdk_pool = state.sdk_sessions.lock().await;
        let interactive_count = flow_sessions.sessions.iter()
            .filter(|s| s.kind == "interactive")
            .count();

        let list: Vec<Value> = flow_sessions
            .sessions
            .iter()
            .map(|s| {
                let proc_k = process_key(&id, &s.session_id);
                let process_alive = sdk_pool.get(&proc_k)
                    .map_or(false, |session| session.is_connected());
                let mut v = json!({
                    "session_id": s.session_id,
                    "summary": s.summary,
                    "message_count": s.message_count,
                    "total_cost": s.total_cost,
                    "created_at": s.created_at,
                    "busy": s.busy,
                    "kind": s.kind,
                    "process_alive": process_alive,
                });
                if let Some(ref fr) = s.flow_run {
                    v["flow_run"] = serde_json::to_value(fr).unwrap_or_default();
                }
                v
            })
            .collect();
        Json(json!({
            "agent_id": id,
            "active_session": flow_sessions.active_session,
            "sessions": list,
            "interactive_count": interactive_count,
            "max_interactive_sessions": MAX_INTERACTIVE_SESSIONS,
        }))
    } else {
        Json(json!({
            "agent_id": id,
            "active_session": "",
            "sessions": [],
            "interactive_count": 0,
            "max_interactive_sessions": MAX_INTERACTIVE_SESSIONS,
        }))
    }
}

/// POST /agents/{id}/sessions — create a new session tab
pub(crate) async fn new_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent = state.agent_repo.get(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "agent not found" })))
    })?;

    let original_working_dir = agent.working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .to_string_lossy()
            .to_string()
    });

    let key = agent_key(&id);
    let new_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Try to create a worktree group for git isolation
    let (working_dir, worktree_group) = match crate::git::create_worktree_group(
        std::path::Path::new(&original_working_dir),
        &new_id,
    ) {
        Ok(group) => {
            let meta = crate::git::WorktreeGroupMeta::from(&group);
            let wt_working_dir = group.shadow_root.to_string_lossy().to_string();
            tracing::info!(
                session_id = %new_id,
                shadow_root = %wt_working_dir,
                repos = group.repos.len(),
                single_repo = group.single_repo,
                "created worktree group for session"
            );
            (wt_working_dir, Some(meta))
        }
        Err(e) => {
            tracing::debug!(
                session_id = %new_id,
                error = %e,
                "no git repos found, session will use original working dir"
            );
            (original_working_dir, None)
        }
    };

    let mut all_sessions = state.interact_sessions.write().await;
    let flow_sessions = all_sessions
        .entry(key.clone())
        .or_insert_with(|| FlowSessions {
            flow_name: agent.name.clone(),
            active_session: String::new(),
            sessions: Vec::new(),
        });

    // Enforce session limit for interactive sessions
    let interactive_count = flow_sessions.sessions.iter()
        .filter(|s| s.kind == "interactive")
        .count();
    if interactive_count >= MAX_INTERACTIVE_SESSIONS {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": format!("session limit reached ({MAX_INTERACTIVE_SESSIONS} interactive sessions max). Close an existing session first.") })),
        ));
    }

    flow_sessions.sessions.push(InteractSession {
        session_id: new_id.clone(),
        summary: String::new(),
        node_id: None,
        working_dir,
        active_pid: None,
        busy: false,
        busy_since: None,
        message_count: 0,
        total_cost: 0.0,
        created_at: now.clone(),
        skills_dir: None,
        kind: "interactive".to_string(),
        flow_run: None,
        worktree_group,
    });
    flow_sessions.active_session = new_id.clone();

    let fs_clone = flow_sessions.clone();
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_session_or_all(&key, &fs_clone, &sessions_snapshot);

    Ok(Json(json!({ "session_id": new_id, "created_at": now })))
}

/// DELETE /agents/{id}/sessions/{session_id}
pub(crate) async fn delete_session(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        let proc_k = process_key(&id, &session_id);
        if let Some(mut session) = sdk_pool.remove(&proc_k) {
            if let Err(e) = session.disconnect().await {
                tracing::warn!(error = %e, "failed to disconnect SDK session on delete");
            }
        }
    }

    let mut all_sessions = state.interact_sessions.write().await;

    let active_after = {
        let flow_sessions = all_sessions.get_mut(&key).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this agent" })))
        })?;

        if flow_sessions.sessions.len() <= 1 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "cannot delete the last session" })),
            ));
        }

        if let Some(session) = flow_sessions.get_session(&session_id) {
            if let Some(pid) = session.active_pid {
                kill_pid(pid);
            }
            // Clean up worktree group if present
            if let Some(ref wt_meta) = session.worktree_group {
                let group = wt_meta.to_worktree_group();
                if let Err(e) = crate::git::remove_worktree_group(&group) {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "failed to remove worktree group on session delete"
                    );
                } else {
                    tracing::info!(session_id = %session_id, "removed worktree group");
                }
            }
        }

        flow_sessions.sessions.retain(|s| s.session_id != session_id);

        if flow_sessions.active_session == session_id {
            if let Some(last) = flow_sessions.sessions.last() {
                flow_sessions.active_session = last.session_id.clone();
            }
        }

        flow_sessions.active_session.clone()
    };

    let fs_clone = all_sessions.get(&key).cloned();
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    if let Some(ref fs) = fs_clone {
        state.save_session_or_all(&key, fs, &sessions_snapshot);
    } else {
        state.save_sessions_to_disk(&sessions_snapshot);
    }

    Ok(Json(json!({
        "deleted": true,
        "active_session": active_after,
    })))
}

/// POST /agents/{id}/chat/stop
pub(crate) async fn stop_chat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<StopRequest>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);

    let target_sid = {
        let sessions = state.interact_sessions.read().await;
        body.and_then(|b| b.session_id.clone())
            .or_else(|| sessions.get(&key).map(|fs| fs.active_session.clone()))
    };

    let sid = target_sid.clone().unwrap_or_default();

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        let proc_key = process_key(&id, &sid);
        if let Some(mut session) = sdk_pool.remove(&proc_key) {
            if let Err(e) = session.disconnect().await {
                tracing::warn!(error = %e, "failed to disconnect SDK session on stop");
            }
        }
    }

    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&key) {
        let sid = target_sid.unwrap_or_else(|| flow_sessions.active_session.clone());

        if let Some(session) = flow_sessions.get_session_mut(&sid) {
            if let Some(pid) = session.active_pid.take() {
                kill_pid(pid);
            }
            session.busy = false;
            session.busy_since = None;
        }
    }
    Ok(Json(json!({ "status": "stopped" })))
}

/// GET /agents/{id}/sessions/{session_id}/status — detailed session status
pub(crate) async fn session_status(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);
    let sessions = state.interact_sessions.read().await;
    let flow_sessions = sessions.get(&key).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this agent" })))
    })?;
    let session = flow_sessions.get_session(&session_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "session not found" })))
    })?;

    let proc_k = process_key(&id, &session_id);
    let sdk_pool = state.sdk_sessions.lock().await;
    let process_alive = sdk_pool.get(&proc_k)
        .map_or(false, |session| session.is_connected());

    Ok(Json(json!({
        "session_id": session_id,
        "busy": session.busy,
        "busy_since": session.busy_since.map(|t| t.to_rfc3339()),
        "process_alive": process_alive,
        "message_count": session.message_count,
        "total_cost": session.total_cost,
    })))
}

/// GET /agents/{id}/sessions/{session_id}/git — git status snapshot
pub(crate) async fn git_status(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);
    let sessions = state.interact_sessions.read().await;
    let flow_sessions = sessions.get(&key).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this agent" })))
    })?;
    let session = flow_sessions.get_session(&session_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "session not found" })))
    })?;

    let wt_meta = session.worktree_group.as_ref().ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "no git integration for this session" })))
    })?;

    let snapshot = crate::git::snapshot_from_meta(wt_meta);
    Ok(Json(serde_json::to_value(&snapshot).unwrap_or_default()))
}

/// POST /agents/{id}/sessions/{session_id}/kill — force-kill session process
pub(crate) async fn kill_session(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);

    // Disconnect SDK session
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        let proc_k = process_key(&id, &session_id);
        if let Some(mut session) = sdk_pool.remove(&proc_k) {
            if let Err(e) = session.disconnect().await {
                tracing::warn!(error = %e, "failed to disconnect SDK session on kill");
            }
        }
    }

    // Clear busy flag
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&key) {
        if let Some(session) = flow_sessions.get_session_mut(&session_id) {
            if let Some(pid) = session.active_pid.take() {
                kill_pid(pid);
            }
            session.busy = false;
            session.busy_since = None;
        }
    }

    let fs_clone = all_sessions.get(&key).cloned();
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    if let Some(ref fs) = fs_clone {
        state.save_session_or_all(&key, fs, &sessions_snapshot);
    } else {
        state.save_sessions_to_disk(&sessions_snapshot);
    }

    Ok(Json(json!({ "status": "killed" })))
}

#[derive(Deserialize)]
#[allow(dead_code)] // Fields used for deserialization; SDK image support pending
pub(crate) struct ImageAttachment {
    pub media_type: String,
    pub data: String, // base64-encoded
}

#[derive(Deserialize)]
pub(crate) struct ChatRequest {
    pub prompt: String,
    pub session_id: Option<String>,
    /// Optional flow context for .skills/ generation (flow_id + node_id).
    pub flow_id: Option<String>,
    pub node_id: Option<String>,
    /// Optional image attachments (base64-encoded).
    pub images: Option<Vec<ImageAttachment>>,
}

#[derive(Deserialize)]
pub(crate) struct StopRequest {
    pub session_id: Option<String>,
}

fn build_sdk_config(
    agent: &crate::agents::Agent,
    session_id: &str,
    working_dir: &str,
    is_new: bool,
    system_prompt: Option<&str>,
    creds: crate::api::local_auth::ResolvedCredentials,
) -> SessionConfig {
    let permission_mode = if agent.permissions.is_empty() {
        Some("bypassPermissions".to_string())
    } else {
        Some("default".to_string())
    };

    SessionConfig {
        cwd: Some(working_dir.to_string()),
        system_prompt: system_prompt.map(String::from),
        allowed_tools: agent.permissions.clone(),
        permission_mode,
        session_id: if is_new { Some(session_id.to_string()) } else { None },
        resume: if !is_new { Some(session_id.to_string()) } else { None },
        include_partial_messages: true,
        api_key: creds.api_key,
        oauth_token: creds.oauth_token,
    }
}

/// Chat stream — runs Claude Code CLI on the user's dedicated VM via SSH.
fn chat_sdk_stream(
    state: AppState,
    id: String,
    prompt: String,
    target_session_id: String,
    is_new: bool,
    working_dir: String,
    system_prompt: Option<String>,
    agent: crate::agents::Agent,
    user_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let key_for_stream = agent_key(&id);
        let proc_key = process_key(&id, &target_session_id);
        let sessions_ref = state.interact_sessions.clone();
        let sessions_path = state.sessions_path.clone();
        let session_streams = state.session_streams.clone();
        let chat_event_buffers = state.chat_event_buffers.clone();
        let data_dir = state.data_dir.clone();

        // Resolve user's VM SSH port
        let ssh_port = {
            let store = state.user_store.read().await;
            store.users.values()
                .find(|u| u.id == user_id)
                .and_then(|u| u.ssh_port)
        };

        let Some(ssh_port) = ssh_port else {
            yield Ok(Event::default().event("error").data(
                serde_json::to_string(&json!({"message": "No VM provisioned. Set your OAuth token in profile settings to create a VM."})).unwrap()
            ));
            // Clear busy flag
            let mut all_sessions = sessions_ref.write().await;
            if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                if let Some(s) = fs.get_session_mut(&target_session_id) {
                    s.busy = false;
                    s.busy_since = None;
                }
            }
            return;
        };

        let vm_client = crate::vm_manager::VmManagerClient::new((*state.http_client).clone());

        // Build claude CLI command with all agent options
        let escaped_prompt = prompt.replace('\'', "'\\''");
        let mut claude_args = vec![
            "claude".to_string(),
            "-p".to_string(),
            format!("'{escaped_prompt}'"),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ];

        // Session management
        if is_new {
            claude_args.push("--session-id".to_string());
            claude_args.push(target_session_id.clone());
        } else {
            claude_args.push("--resume".to_string());
            claude_args.push(target_session_id.clone());
        }

        // System prompt
        if let Some(ref sys_prompt) = system_prompt {
            let escaped_sys = sys_prompt.replace('\'', "'\\''");
            claude_args.push("--append-system-prompt".to_string());
            claude_args.push(format!("'{escaped_sys}'"));
        }

        // Model
        if let Some(ref model) = agent.model {
            claude_args.push("--model".to_string());
            claude_args.push(model.clone());
        }

        // Effort level
        if let Some(ref effort) = agent.effort {
            claude_args.push("--effort".to_string());
            claude_args.push(effort.clone());
        }

        // Budget limit
        if let Some(budget) = agent.max_budget_usd {
            claude_args.push("--max-budget-usd".to_string());
            claude_args.push(format!("{:.2}", budget));
        }

        // Turn limit
        if let Some(turns) = agent.max_turns {
            claude_args.push("--max-turns".to_string());
            claude_args.push(turns.to_string());
        }

        // Permission mode
        if let Some(ref mode) = agent.permission_mode {
            claude_args.push("--permission-mode".to_string());
            claude_args.push(mode.clone());
        }

        // Allowed tools (auto-approved)
        if !agent.allowed_tools.is_empty() {
            claude_args.push("--allowedTools".to_string());
            claude_args.push(agent.allowed_tools.join(","));
        } else if !agent.permissions.is_empty() {
            claude_args.push("--allowedTools".to_string());
            claude_args.push(agent.permissions.join(","));
        }

        // Disallowed tools
        if !agent.disallowed_tools.is_empty() {
            claude_args.push("--disallowedTools".to_string());
            claude_args.push(agent.disallowed_tools.join(","));
        }

        // Restrict available tools
        if !agent.tools.is_empty() {
            claude_args.push("--tools".to_string());
            claude_args.push(agent.tools.join(","));
        }

        // Additional directories
        for dir in &agent.add_dirs {
            claude_args.push("--add-dir".to_string());
            claude_args.push(dir.clone());
        }

        // Sub-agents
        if !agent.subagents.is_empty() {
            if let Ok(agents_json) = serde_json::to_string(&agent.subagents) {
                let escaped = agents_json.replace('\'', "'\\''");
                claude_args.push("--agents".to_string());
                claude_args.push(format!("'{escaped}'"));
            }
        }

        // MCP servers
        if let Some(ref mcp) = agent.mcp_config {
            if let Ok(mcp_json) = serde_json::to_string(mcp) {
                let escaped = mcp_json.replace('\'', "'\\''");
                claude_args.push("--mcp-config".to_string());
                claude_args.push(format!("'{escaped}'"));
            }
        }

        // Git worktree isolation
        if agent.use_worktree {
            claude_args.push("-w".to_string());
        }

        // Custom settings
        if let Some(ref settings) = agent.custom_settings {
            if let Ok(settings_json) = serde_json::to_string(settings) {
                let escaped = settings_json.replace('\'', "'\\''");
                claude_args.push("--settings".to_string());
                claude_args.push(format!("'{escaped}'"));
            }
        }

        let ssh_command = claude_args.join(" ");
        tracing::info!(
            ssh_port,
            session_id = %target_session_id,
            "running claude on user VM via SSH"
        );

        yield Ok(Event::default().event("system").data(
            serde_json::to_string(&json!({"message": "Running on your VM..."})).unwrap()
        ));

        // Spawn SSH process to user's VM
        let child_result = vm_client.ssh_stream(ssh_port, &ssh_command).await;
        let mut child = match child_result {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "SSH to user VM failed");
                let mut all_sessions = sessions_ref.write().await;
                if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                    if let Some(s) = fs.get_session_mut(&target_session_id) {
                        s.busy = false;
                        s.busy_since = None;
                    }
                }
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("VM connection failed: {e}")})).unwrap()
                ));
                return;
            }
        };

        // Create broadcast channel + event buffer for reconnection support
        let (bc_tx, _) = broadcast::channel::<String>(1024);
        {
            let mut streams = session_streams.lock().await;
            streams.insert(proc_key.clone(), bc_tx.clone());
        }
        {
            let mut buffers = chat_event_buffers.lock().await;
            buffers.insert(proc_key.clone(), Vec::new());
        }

        let logs_dir = data_dir.join("session_logs");
        let _ = std::fs::create_dir_all(&logs_dir);
        let log_path = logs_dir.join(format!("{target_session_id}.jsonl"));

        // Background task: read SSH stdout (stream-json from claude CLI) and broadcast
        {
            let bc_tx = bc_tx.clone();
            let sessions_ref = sessions_ref.clone();
            let session_streams = session_streams.clone();
            let chat_event_buffers = chat_event_buffers.clone();
            let proc_key = proc_key.clone();
            let key_for_bg = key_for_stream.clone();
            let sid_for_bg = target_session_id.clone();
            let sessions_path = sessions_path.clone();
            let log_path = log_path.clone();
            let mongo_db_bg = state.mongo_db.clone();

            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};

                let mut session_cost: f64 = 0.0;

                let append_log = |line: &str| {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path)
                    {
                        let _ = writeln!(f, "{}", line);
                    }
                };
                append_log(&format!("user:{}", serde_json::to_string(&json!({"text": prompt})).unwrap_or_default()));

                // Read stdout line by line (stream-json from claude CLI)
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.is_empty() { continue; }

                        // Parse stream-json output from claude CLI
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&line) {
                            let event_type = json_val.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                            let sse_event = match event_type {
                                "content_block_delta" => {
                                    if let Some(text) = json_val.pointer("/delta/text").and_then(|v| v.as_str()) {
                                        if !text.is_empty() {
                                            Some(format!("text:{}", serde_json::to_string(&json!({"text": text})).unwrap()))
                                        } else { None }
                                    } else { None }
                                }
                                "content_block_start" => {
                                    if json_val.pointer("/content_block/type").and_then(|v| v.as_str()) == Some("tool_use") {
                                        let tool = json_val.pointer("/content_block/name").and_then(|v| v.as_str()).unwrap_or("?");
                                        Some(format!("tool_use:{}", serde_json::to_string(&json!({"tool": tool, "input": ""})).unwrap()))
                                    } else { None }
                                }
                                "result" => {
                                    session_cost = json_val.get("total_cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let result_text = json_val.get("result").and_then(|v| v.as_str()).unwrap_or("");
                                    let turns = json_val.get("num_turns").and_then(|v| v.as_u64()).unwrap_or(0);
                                    Some(format!("result:{}", serde_json::to_string(&json!({"text": result_text, "cost": session_cost, "turns": turns})).unwrap()))
                                }
                                "assistant" => {
                                    // Full assistant message — extract text blocks
                                    if let Some(content) = json_val.pointer("/message/content").and_then(|c| c.as_array()) {
                                        for block in content {
                                            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                                    let event_str = format!("text:{}", serde_json::to_string(&json!({"text": text})).unwrap());
                                                    let _ = bc_tx.send(event_str.clone());
                                                    append_log(&event_str);
                                                    let mut buffers = chat_event_buffers.lock().await;
                                                    if let Some(buf) = buffers.get_mut(&proc_key) { buf.push(event_str); }
                                                }
                                            }
                                        }
                                    }
                                    None
                                }
                                _ => None,
                            };

                            if let Some(event_str) = sse_event {
                                let is_result = event_str.starts_with("result:");
                                let _ = bc_tx.send(event_str.clone());
                                append_log(&event_str);
                                {
                                    let mut buffers = chat_event_buffers.lock().await;
                                    if let Some(buf) = buffers.get_mut(&proc_key) { buf.push(event_str); }
                                }
                                if is_result { break; }
                            }
                        } else {
                            // Non-JSON line — send as text
                            let event_str = format!("text:{}", serde_json::to_string(&json!({"text": line})).unwrap());
                            let _ = bc_tx.send(event_str.clone());
                            append_log(&event_str);
                            let mut buffers = chat_event_buffers.lock().await;
                            if let Some(buf) = buffers.get_mut(&proc_key) { buf.push(event_str); }
                        }
                    }
                }

                // Wait for SSH process to exit
                let _ = child.wait().await;

                // Send done event
                let done_data = serde_json::to_string(&json!({"exit_code": 0})).unwrap();
                let done_event = format!("done:{done_data}");
                let _ = bc_tx.send(done_event.clone());
                append_log(&done_event);
                {
                    let mut buffers = chat_event_buffers.lock().await;
                    if let Some(buf) = buffers.get_mut(&proc_key) { buf.push(done_event); }
                }

                // Update session state
                {
                    let mut all_sessions = sessions_ref.write().await;
                    if let Some(fs) = all_sessions.get_mut(&key_for_bg) {
                        if let Some(s) = fs.get_session_mut(&sid_for_bg) {
                            s.busy = false;
                            s.busy_since = None;
                            s.message_count += 1;
                            s.total_cost += session_cost;
                        }
                    }
                    if let Some(ref db) = mongo_db_bg {
                        if let Some(fs) = all_sessions.get(&key_for_bg) {
                            let db = db.clone();
                            let key = key_for_bg.clone();
                            let fs = fs.clone();
                            tokio::spawn(async move { db.save_session(&key, &fs).await; });
                        }
                    } else {
                        let sessions_snapshot = all_sessions.clone();
                        crate::api::save_sessions(&sessions_path, &sessions_snapshot);
                    }
                    drop(all_sessions);
                }

                // Cleanup after delay
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                {
                    let mut streams = session_streams.lock().await;
                    streams.remove(&proc_key);
                }
                {
                    let mut buffers = chat_event_buffers.lock().await;
                    buffers.remove(&proc_key);
                }
            });
        }

        // Subscribe to the broadcast channel and yield events as SSE
        let mut rx = bc_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(event_str) => {
                    if let Some((event_type, data)) = event_str.split_once(':') {
                        yield Ok(Event::default().event(event_type).data(data));
                        if event_type == "done" {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "agent chat SDK broadcast subscriber lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    }
}

/// POST /agents/{id}/chat — SSE stream for agent chat
pub(crate) async fn chat(
    auth: crate::api::local_auth::AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ChatRequest>,
) -> Result<
    Sse<axum::response::sse::KeepAliveStream<BoxSseStream>>,
    (StatusCode, Json<Value>),
> {
    let agent = state.agent_repo.get(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "agent not found" })))
    })?;

    let prompt = body.prompt;
    let images = body.images.unwrap_or_default();
    if prompt.trim().is_empty() && images.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "prompt is required" })),
        ));
    }

    let append_system_prompt = agent.append_system_prompt.clone();

    let default_working_dir = agent.working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .to_string_lossy()
            .to_string()
    });

    let key = agent_key(&id);

    // Load the flow if flow context was provided (for .skills/ generation)
    let flow_context = if let (Some(flow_id), Some(node_id)) = (&body.flow_id, &body.node_id) {
        state.flow_repo.get_flow(flow_id).await.map(|flow| (flow, node_id.clone()))
    } else {
        None
    };

    // Pre-create worktree group if this agent has no sessions yet (outside write lock)
    let needs_new_entry = {
        let sessions = state.interact_sessions.read().await;
        !sessions.contains_key(&key)
    };
    let pre_session_id = Uuid::new_v4().to_string();
    let prepared_worktree: Option<(String, crate::git::WorktreeGroupMeta)> = if needs_new_entry {
        match crate::git::create_worktree_group(
            std::path::Path::new(&default_working_dir),
            &pre_session_id,
        ) {
            Ok(group) => {
                let meta = crate::git::WorktreeGroupMeta::from(&group);
                let wt_dir = group.shadow_root.to_string_lossy().to_string();
                tracing::info!(
                    session_id = %pre_session_id,
                    shadow_root = %wt_dir,
                    repos = group.repos.len(),
                    single_repo = group.single_repo,
                    "created worktree group for auto-created session"
                );
                Some((wt_dir, meta))
            }
            Err(e) => {
                tracing::debug!(error = %e, "no git repos for auto-created session");
                None
            }
        }
    } else {
        None
    };

    // Look up or create the session
    let (target_session_id, is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;
        let mut used_prepared_worktree = false;

        let flow_sessions = all_sessions
            .entry(key.clone())
            .or_insert_with(|| {
                used_prepared_worktree = true;
                let (wdir, wt_group) = match &prepared_worktree {
                    Some((dir, meta)) => (dir.clone(), Some(meta.clone())),
                    None => (default_working_dir.clone(), None),
                };
                FlowSessions {
                    flow_name: agent.name.clone(),
                    active_session: pre_session_id.clone(),
                    sessions: vec![InteractSession {
                        session_id: pre_session_id.clone(),
                        summary: make_summary(&prompt),
                        node_id: None,
                        working_dir: wdir,
                        active_pid: None,
                        busy: false,
                        busy_since: None,
                        message_count: 0,
                        total_cost: 0.0,
                        created_at: Utc::now().to_rfc3339(),
                        skills_dir: None,
                        kind: "interactive".to_string(),
                        flow_run: None,
                        worktree_group: wt_group,
                    }],
                }
            });

        // Race condition: if another request created the entry between our read
        // and write locks, clean up the unused worktree group
        if !used_prepared_worktree {
            if let Some((_, ref meta)) = prepared_worktree {
                let group = meta.to_worktree_group();
                let _ = crate::git::remove_worktree_group(&group);
            }
        }

        let target_sid = body.session_id
            .unwrap_or_else(|| flow_sessions.active_session.clone());

        let session = match flow_sessions.get_session_mut(&target_sid) {
            Some(s) => s,
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": format!("session {} not found", target_sid) })),
                ));
            }
        };

        if session.busy {
            // Check for stale busy — SDK session might be disconnected
            let proc_k = process_key(&id, &session.session_id);
            let is_stale = {
                let sdk_pool = state.sdk_sessions.lock().await;
                if let Some(sdk_session) = sdk_pool.get(&proc_k) {
                    !sdk_session.is_connected()
                } else {
                    // No session in pool — check if it's been busy too long
                    session.busy_since
                        .map(|since| chrono::Utc::now().signed_duration_since(since).to_std().unwrap_or_default() > STALE_BUSY_TIMEOUT)
                        .unwrap_or(true) // No timestamp = definitely stale
                }
            };

            if is_stale {
                tracing::warn!(
                    session_id = %session.session_id,
                    "auto-recovering stale busy session"
                );
                let proc_k = process_key(&id, &session.session_id);
                let mut sdk_pool = state.sdk_sessions.lock().await;
                sdk_pool.remove(&proc_k);
                drop(sdk_pool);
                session.busy = false;
                session.busy_since = None;
                session.active_pid = None;
            } else {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "session is busy processing a previous message" })),
                ));
            }
        }

        let is_new = session.message_count == 0;

        if is_new && session.summary.is_empty() {
            session.summary = make_summary(&prompt);
        }

        session.busy = true;
        session.busy_since = Some(chrono::Utc::now());
        let sid = session.session_id.clone();
        let wdir = session.working_dir.clone();

        flow_sessions.active_session = sid.clone();

        let fs_clone = flow_sessions.clone();
        let sessions_snapshot = all_sessions.clone();
        drop(all_sessions);
        state.save_session_or_all(&key, &fs_clone, &sessions_snapshot);

        (sid, is_new, wdir)
    };

    // On first message: generate .skills/ context files and build system prompt
    let data_dir = state.data_dir.clone();

    // Generate .claude/settings.local.json with hook URLs for this session.
    // System hooks (permission gate, file-change broadcast, stop notification) are
    // always present. Per-agent hooks from agent.hooks are merged in — agent hook
    // groups are appended *after* the system groups for each event so they can
    // layer on additional behaviour (e.g. Bugs Bunny's read-only guard).
    {
        let claude_dir = std::path::Path::new(&working_dir).join(".claude");
        let _ = std::fs::create_dir_all(&claude_dir);
        let settings_path = claude_dir.join("settings.local.json");

        // Read existing settings if present, to merge hooks in
        let mut settings: serde_json::Value = if settings_path.exists() {
            std::fs::read_to_string(&settings_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };

        let port = state.server_port;
        let sid = &target_session_id;
        let hook_base = format!("http://localhost:{port}/api/hooks");

        // Start with the system hooks — always present.
        // Hook config: 3 levels — event -> matcher group -> hook handlers
        // See https://code.claude.com/docs/en/hooks
        //
        // NOTE: PermissionRequest hooks do NOT fire in stream-json mode.
        // We use PreToolUse as the permission gate instead.
        let mut hooks_map: serde_json::Map<String, Value> = serde_json::Map::new();

        hooks_map.insert("PreToolUse".into(), json!([{
            "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
            "hooks": [{
                "type": "http",
                "url": format!("{hook_base}/pre-tool-use?session_id={sid}"),
                "timeout": 130
            }]
        }]));
        hooks_map.insert("PostToolUse".into(), json!([{
            "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
            "hooks": [{
                "type": "http",
                "url": format!("{hook_base}/post-tool-use?session_id={sid}")
            }]
        }]));
        hooks_map.insert("Stop".into(), json!([{
            "hooks": [{
                "type": "http",
                "url": format!("{hook_base}/stop?session_id={sid}")
            }]
        }]));

        // Merge per-agent hooks: append agent hook groups after system groups.
        if !agent.hooks.is_empty() {
            for (event, agent_groups) in &agent.hooks {
                let agent_groups_json = serde_json::to_value(agent_groups).unwrap_or(json!([]));
                if let Some(existing) = hooks_map.get_mut(event) {
                    // Event already has system hooks — append agent groups
                    if let Some(arr) = existing.as_array_mut() {
                        if let Some(extra) = agent_groups_json.as_array() {
                            arr.extend(extra.iter().cloned());
                        }
                    }
                } else {
                    // Event only defined by agent (e.g. SessionStart, UserPromptSubmit)
                    hooks_map.insert(event.clone(), agent_groups_json);
                }
            }
            tracing::info!(
                agent_id = %id,
                agent_hook_events = agent.hooks.len(),
                "merged per-agent hooks into settings"
            );
        }

        settings["hooks"] = Value::Object(hooks_map);

        if let Ok(json_str) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(&settings_path, json_str);
            tracing::info!(
                path = %settings_path.display(),
                session_id = %sid,
                "wrote .claude/settings.local.json with hook URLs"
            );
        }
    }

    let system_prompt = if is_new {
        let has_flow_context = flow_context.is_some();

        // Only generate .skills/ for flow-context agents (executor nodes in flows)
        if has_flow_context {
            let skills_dir = std::path::Path::new(&working_dir).join(".skills");
            let _ = std::fs::create_dir_all(&skills_dir);

            // 1. Copy AGENT.md from project root to .skills/
            let agent_md_candidates = [
                std::path::PathBuf::from("AGENT.md"),
                data_dir.join("AGENT.md"),
            ];
            for candidate in &agent_md_candidates {
                if candidate.exists() {
                    let _ = std::fs::copy(candidate, skills_dir.join("AGENT.md"));
                    break;
                }
            }

            // 2. Generate Skill.md and workflow.json
            if let Some((ref flow, ref node_id)) = flow_context {
                let skill_md = build_workflow_context_md(flow, node_id);
                let _ = std::fs::write(skills_dir.join("Skill.md"), &skill_md);

                if let Ok(flow_json) = serde_json::to_string_pretty(flow) {
                    let _ = std::fs::write(skills_dir.join("workflow.json"), &flow_json);
                }

                // Sync user-uploaded attachments to .skills/
                let att_dir = attachments_path(&data_dir, &flow.id, node_id);
                if att_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&att_dir) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_file() {
                                let _ = std::fs::copy(&p, skills_dir.join(entry.file_name()));
                            }
                        }
                    }
                }
            }

            // 3. Store skills_dir path in session
            {
                let mut all_sessions = state.interact_sessions.write().await;
                if let Some(fs) = all_sessions.get_mut(&key) {
                    if let Some(s) = fs.get_session_mut(&target_session_id) {
                        s.skills_dir = Some(skills_dir.to_string_lossy().to_string());
                    }
                }
                let fs_clone = all_sessions.get(&key).cloned();
                let sessions_snapshot = all_sessions.clone();
                drop(all_sessions);
                if let Some(ref fs) = fs_clone {
                    state.save_session_or_all(&key, fs, &sessions_snapshot);
                } else {
                    state.save_sessions_to_disk(&sessions_snapshot);
                }
            }

            // 4. Build scoped system prompt for flow executor agents
            let mut sys_prompt = format!(
                "You are \"{agent_name}\", an AI agent.\n\n\
                 CRITICAL RULES — FOLLOW EXACTLY:\n\n\
                 1. FIRST: Read .skills/Skill.md and .skills/AGENT.md — these contain ALL your context.\n\
                 2. SCOPE LOCKED: Do NOT explore, search, or read ANY files outside .skills/ directory.\n\
                    No find, no grep, no ls, no bash exploration, no reading source code, no git commands.\n\
                    Your .skills/ files already contain your workflow context, pipeline info, and rules.\n\
                 3. ANSWER FROM CONTEXT: When asked about the workflow, answer from .skills/Skill.md\n\
                    and .skills/workflow.json — do NOT go looking for more information.\n\
                 4. IF THE USER WANTS MORE: Only if the user explicitly says \"read the codebase\",\n\
                    \"explore the project\", or similar — THEN you may expand scope. Not before.\n\
                 5. BE EFFICIENT: Short answers. No preamble. No filler. Batch tool calls.\n\
                    Read .skills/AGENT.md for full efficiency rules.\n\n\
                 Files in .skills/:\n\
                 - .skills/AGENT.md — agent rules, scope boundaries, efficiency rules\n\
                 - .skills/Skill.md — your position in the pipeline and workflow context\n\
                 - .skills/workflow.json — full workflow definition",
                agent_name = agent.name,
            );

            if let Some(ref extra) = append_system_prompt {
                if !extra.is_empty() {
                    sys_prompt.push_str(&format!("\n\n{extra}"));
                }
            }

            Some(sys_prompt)
        } else {
            // Standalone agent — simple system prompt, no .skills/ scope lock
            let mut sys_prompt = format!(
                "You are \"{agent_name}\", an AI assistant. \
                 Your working directory is: {working_dir}\n\
                 Be efficient: short answers, no preamble, batch tool calls when possible.",
                agent_name = agent.name,
                working_dir = working_dir,
            );

            if let Some(ref extra) = append_system_prompt {
                if !extra.is_empty() {
                    sys_prompt.push_str(&format!("\n\n{extra}"));
                }
            }

            Some(sys_prompt)
        }
    } else {
        None
    };

    // Use Agent SDK for chat streaming
    let stream = chat_sdk_stream(
        state,
        id,
        prompt,
        target_session_id,
        is_new,
        working_dir,
        system_prompt,
        agent,
        auth.user_id,
    );
    let boxed: BoxSseStream = Box::pin(stream);
    Ok(Sse::new(boxed).keep_alive(
        KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    ))
}

// ---------------------------------------------------------------------------
// Agent chat reconnect endpoint
// ---------------------------------------------------------------------------

/// GET /agents/{id}/sessions/{session_id}/chat/stream — reconnect to an in-flight agent chat stream.
/// Replays buffered events then subscribes to the live broadcast channel.
pub(crate) async fn stream_agent_chat(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);
    let proc_key = process_key(&id, &session_id);

    tracing::info!(
        agent_id = %id,
        session_id = %session_id,
        proc_key = %proc_key,
        "[RECONNECT-DEBUG] stream_agent_chat endpoint HIT"
    );

    // Verify the session exists
    {
        let sessions = state.interact_sessions.read().await;
        let flow_sessions = sessions.get(&key).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this agent" })))
        })?;
        flow_sessions.get_session(&session_id).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "session not found" })))
        })?;
    }

    let session_streams = state.session_streams.clone();
    let chat_event_buffers = state.chat_event_buffers.clone();

    let stream = async_stream::stream! {
        // 1. Replay buffered events (catch-up)
        let buffered_events = {
            let buffers = chat_event_buffers.lock().await;
            let events = buffers.get(&proc_key).cloned().unwrap_or_default();
            tracing::info!(
                proc_key = %proc_key,
                buffered_count = events.len(),
                has_buffer = buffers.contains_key(&proc_key),
                "[RECONNECT-DEBUG] Replay: found buffered events"
            );
            events
        };

        let mut already_done = false;
        for event_str in &buffered_events {
            if let Some((event_type, data)) = event_str.split_once(':') {
                yield Ok(Event::default().event(event_type).data(data));
                if event_type == "done" {
                    already_done = true;
                }
            }
        }
        tracing::info!(
            proc_key = %proc_key,
            already_done,
            replayed = buffered_events.len(),
            "[RECONNECT-DEBUG] Replay complete"
        );

        // 2. If not done, subscribe to broadcast for live events
        if !already_done {
            let rx = {
                let streams = session_streams.lock().await;
                let has_stream = streams.contains_key(&proc_key);
                tracing::info!(
                    proc_key = %proc_key,
                    has_stream,
                    total_streams = streams.len(),
                    all_keys = ?streams.keys().collect::<Vec<_>>(),
                    "[RECONNECT-DEBUG] Looking for broadcast channel"
                );
                streams.get(&proc_key).map(|tx| tx.subscribe())
            };

            if let Some(mut rx) = rx {
                tracing::info!(
                    proc_key = %proc_key,
                    "[RECONNECT-DEBUG] Subscribed to broadcast, starting live relay"
                );
                // Skip events we already replayed from the buffer
                let replay_count = buffered_events.len();
                let mut skipped = 0;

                loop {
                    match rx.recv().await {
                        Ok(event_str) => {
                            // Skip events that were in the buffer at replay time
                            if skipped < replay_count {
                                skipped += 1;
                                continue;
                            }
                            if let Some((event_type, data)) = event_str.split_once(':') {
                                yield Ok(Event::default().event(event_type).data(data));
                                if event_type == "done" {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "agent chat reconnect subscriber lagged");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            } else {
                // No broadcast channel — session is not actively streaming.
                tracing::warn!(
                    proc_key = %proc_key,
                    "[RECONNECT-DEBUG] No broadcast channel found! Session not actively streaming."
                );
                let is_busy = {
                    let sessions = state.interact_sessions.read().await;
                    sessions.get(&key)
                        .and_then(|fs| fs.get_session(&session_id))
                        .map(|s| s.busy)
                        .unwrap_or(false)
                };
                if is_busy {
                    // Stale busy flag — no broadcast means the background task already finished
                    yield Ok(Event::default().event("done").data(
                        serde_json::to_string(&json!({"exit_code": 0, "reconnected": false})).unwrap()
                    ));
                } else {
                    yield Ok(Event::default().event("done").data(
                        serde_json::to_string(&json!({"exit_code": 0, "reconnected": false})).unwrap()
                    ));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

// ---------------------------------------------------------------------------
// Flow-run session streaming endpoints
// ---------------------------------------------------------------------------

/// GET /agents/{id}/sessions/{session_id}/stream — SSE stream for flow-run session
pub(crate) async fn stream_session_log(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);

    // Verify the session exists and is a flow_run session
    {
        let sessions = state.interact_sessions.read().await;
        let flow_sessions = sessions.get(&key).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this agent" })))
        })?;
        let session = flow_sessions.get_session(&session_id).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "session not found" })))
        })?;
        if session.kind != "flow_run" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "session is not a flow_run session" })),
            ));
        }
    }

    let logs_dir = state.data_dir.join("session_logs");
    let log_path = logs_dir.join(format!("{session_id}.jsonl"));
    let session_streams = state.session_streams.clone();
    let interact_sessions = state.interact_sessions.clone();
    let agent_key_owned = key;

    let stream = async_stream::stream! {
        // 1. Replay existing lines from JSONL file (catch-up)
        if log_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&log_path).await {
                for line in content.lines() {
                    if !line.is_empty() {
                        yield Ok(Event::default().event("line").data(line));
                    }
                }
            }
        }

        // 2. Subscribe to broadcast for live lines (if session is still busy)
        let is_busy = {
            let sessions = interact_sessions.read().await;
            sessions.get(&agent_key_owned)
                .and_then(|fs| fs.get_session(&session_id))
                .map(|s| s.busy)
                .unwrap_or(false)
        };

        if is_busy {
            let mut rx = {
                let streams = session_streams.lock().await;
                streams.get(&session_id).map(|tx| tx.subscribe())
            };

            if let Some(ref mut rx) = rx {
                loop {
                    match rx.recv().await {
                        Ok(line) => {
                            yield Ok(Event::default().event("line").data(line));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(session_id = %session_id, skipped = n, "session stream subscriber lagged");
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }

        yield Ok(Event::default().event("done").data(
            serde_json::to_string(&json!({"status": "complete"})).unwrap()
        ));
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

/// GET /agents/{id}/sessions/{session_id}/log — full JSONL log as JSON array
pub(crate) async fn get_session_log(
    State(state): State<AppState>,
    Path((_id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let logs_dir = state.data_dir.join("session_logs");
    let log_path = logs_dir.join(format!("{session_id}.jsonl"));

    if !log_path.exists() {
        return Ok(Json(json!({ "lines": [] })));
    }

    let content = tokio::fs::read_to_string(&log_path).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("failed to read log: {e}") })))
    })?;

    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    Ok(Json(json!({ "lines": lines })))
}

// ---------------------------------------------------------------------------
// Skill context generation helpers
// ---------------------------------------------------------------------------

/// Brief config summary for a node, extracting the most important field per kind.
fn node_config_summary(node: &Node) -> String {
    match node.kind.as_str() {
        "cron" => {
            let schedule = node.config.get("schedule").and_then(|v| v.as_str()).unwrap_or("?");
            format!("schedule: {schedule}")
        }
        "rss" => {
            let url = node.config.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            let limit = node.config.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
            format!("url: {url}, limit: {limit}")
        }
        "web-scrape" | "web-scraper" => {
            let url = node.config.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            format!("url: {url}")
        }
        "github-merged-prs" => {
            let repos = node.config.get("repos").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "?".into());
            format!("repos: {repos}")
        }
        "github-pr" => {
            let poll = node.config.get("poll_interval").and_then(|v| v.as_u64()).unwrap_or(60);
            format!("poll: {poll}s")
        }
        "market-data" => "(fetches BTC/ETH, Fear & Greed, S&P 500)".into(),
        "claude-code" => {
            let prompt = node.config.get("prompt").and_then(|v| v.as_str()).unwrap_or("(inline)");
            format!("prompt: {prompt}")
        }
        "slack" => {
            let channel = node.config.get("channel").and_then(|v| v.as_str()).unwrap_or("?");
            let method = if node.config.get("bot_token_env").and_then(|v| v.as_str()).is_some() {
                "bot"
            } else {
                "webhook"
            };
            format!("{method}, channel: {channel}")
        }
        "notion" => {
            let db = node.config.get("database_id").and_then(|v| v.as_str()).unwrap_or("?");
            format!("database: {db}")
        }
        "manual" => "(triggered manually)".into(),
        "webhook" => {
            let path = node.config.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("path: {path}")
        }
        _ => "(no config summary)".into(),
    }
}

/// Walk edges to find all upstream node IDs.
fn find_upstream(node_id: &str, edges: &[Edge], depth: usize) -> Vec<String> {
    if depth > 20 { return vec![]; }
    let direct: Vec<String> = edges.iter()
        .filter(|e| e.target == node_id)
        .map(|e| e.source.clone())
        .collect();
    let mut all = direct.clone();
    for parent in &direct {
        all.extend(find_upstream(parent, edges, depth + 1));
    }
    all
}

/// Walk edges to find all downstream node IDs.
fn find_downstream(node_id: &str, edges: &[Edge], depth: usize) -> Vec<String> {
    if depth > 20 { return vec![]; }
    let direct: Vec<String> = edges.iter()
        .filter(|e| e.source == node_id)
        .map(|e| e.target.clone())
        .collect();
    let mut all = direct.clone();
    for child in &direct {
        all.extend(find_downstream(child, edges, depth + 1));
    }
    all
}

/// Build the Skill.md content describing this executor's position in the pipeline.
fn build_workflow_context_md(flow: &Flow, node_id: &str) -> String {
    let node_map: std::collections::HashMap<&str, &Node> = flow.nodes.iter()
        .map(|n| (n.id.as_str(), n))
        .collect();

    let current_node = node_map.get(node_id);
    let current_label = current_node.map(|n| n.label.as_str()).unwrap_or("Unknown");

    let upstream_ids = find_upstream(node_id, &flow.edges, 0);
    let downstream_ids = find_downstream(node_id, &flow.edges, 0);

    let mut pipeline_lines = Vec::new();

    for n in &flow.nodes {
        if n.node_type == NodeType::Trigger {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("  {marker}Trigger: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    for n in &flow.nodes {
        if n.node_type == NodeType::Source {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("    -> {marker}Source: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    let executors: Vec<&Node> = flow.nodes.iter()
        .filter(|n| n.node_type == NodeType::Executor)
        .collect();
    for n in &executors {
        if n.id == node_id {
            pipeline_lines.push(format!("    -> **YOU: {} (claude-code)**", n.label));
        } else {
            pipeline_lines.push(format!("    -> Executor: {} ({}) -- {}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    for n in &flow.nodes {
        if n.node_type == NodeType::Sink {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("    -> {marker}Sink: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    let mut upstream_table = String::new();
    for uid in &upstream_ids {
        if let Some(n) = node_map.get(uid.as_str()) {
            upstream_table.push_str(&format!(
                "| {} | {} | {} |\n",
                n.label, n.kind, node_config_summary(n)
            ));
        }
    }

    let mut downstream_table = String::new();
    for did in &downstream_ids {
        if let Some(n) = node_map.get(did.as_str()) {
            downstream_table.push_str(&format!(
                "| {} | {} | {} |\n",
                n.label, n.kind, node_config_summary(n)
            ));
        }
    }

    let mut executor_list = String::new();
    for (i, ex) in executors.iter().enumerate() {
        let num = format!("E{:02}", i + 1);
        if ex.id == node_id {
            executor_list.push_str(&format!(
                "- **{num}: {} (this node)** -- {}\n",
                ex.label, node_config_summary(ex)
            ));
        } else {
            executor_list.push_str(&format!(
                "- {num}: {} -- {}\n",
                ex.label, node_config_summary(ex)
            ));
        }
    }

    let prompt_path = current_node
        .and_then(|n| n.config.get("prompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("(inline/none)");

    format!(r#"# Workflow Context

## Flow: {flow_name}

{flow_description}

## Your Position in the Pipeline

```
{pipeline}
```

## Upstream Nodes (feeding data into you)

| Node | Kind | Config |
|------|------|--------|
{upstream_table}
## Downstream Nodes (receiving your output)

| Node | Kind | Config |
|------|------|--------|
{downstream_table}
## All Executors in This Flow

{executor_list}
## Your Configuration

- **Label**: {current_label}
- **Prompt path**: {prompt_path}
- **Node ID**: {node_id}
"#,
        flow_name = flow.name,
        flow_description = flow.description,
        pipeline = pipeline_lines.join("\n"),
        upstream_table = upstream_table,
        downstream_table = downstream_table,
        executor_list = executor_list,
        current_label = current_label,
        prompt_path = prompt_path,
        node_id = node_id,
    )
}

