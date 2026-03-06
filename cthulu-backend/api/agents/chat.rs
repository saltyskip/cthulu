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
use crate::api::LiveClaudeProcess;
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

/// Check if the Agent SDK integration is enabled via AGENT_SDK_ENABLED env var.
/// When enabled, new chat sessions use `AgentSession` (SDK) instead of `LiveClaudeProcess`.
fn agent_sdk_enabled() -> bool {
    std::env::var("AGENT_SDK_ENABLED")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
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
        let mut pool = state.live_processes.lock().await;
        let sdk_pool = state.sdk_sessions.lock().await;
        let interactive_count = flow_sessions.sessions.iter()
            .filter(|s| s.kind == "interactive")
            .count();

        let list: Vec<Value> = flow_sessions
            .sessions
            .iter()
            .map(|s| {
                let proc_k = process_key(&id, &s.session_id);
                // Check both legacy and SDK process pools
                let process_alive = if let Some(proc) = pool.get_mut(&proc_k) {
                    !matches!(proc.child.try_wait(), Ok(Some(_)))
                } else if let Some(sdk_session) = sdk_pool.get(&proc_k) {
                    sdk_session.is_connected()
                } else {
                    false
                };
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
        .entry(key)
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

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

    Ok(Json(json!({ "session_id": new_id, "created_at": now })))
}

/// DELETE /agents/{id}/sessions/{session_id}
pub(crate) async fn delete_session(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = agent_key(&id);

    // Remove per-session live process (Drop impl will kill it)
    {
        let mut pool = state.live_processes.lock().await;
        let proc_k = process_key(&id, &session_id);
        pool.remove(&proc_k);
    }

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

    // Kill PTY process for this specific session
    {
        let mut pty_pool = state.pty_processes.lock().await;
        let pty_k = super::terminal::pty_key(&id, Some(&session_id));
        if let Some(mut pty) = pty_pool.remove(&pty_k) {
            let _ = pty.child.kill();
            let _ = pty.child.wait();
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

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

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

    // Remove the persistent process from the pool (Drop impl will kill it)
    {
        let mut pool = state.live_processes.lock().await;
        let proc_key = process_key(&id, &sid);
        pool.remove(&proc_key);
        // Also try legacy agent-level key for backward compat
        pool.remove(&key);
    }

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

    // Kill PTY process — try session-scoped key first, then legacy agent-level key
    {
        let mut pty_pool = state.pty_processes.lock().await;
        let pty_k = match &target_sid {
            Some(sid) => super::terminal::pty_key(&id, Some(sid)),
            None => super::terminal::pty_key(&id, None),
        };
        if let Some(mut pty) = pty_pool.remove(&pty_k) {
            let _ = pty.child.kill();
            let _ = pty.child.wait();
        }
        // Also try legacy key (agent-level, no session) for backward compat
        let legacy_key = key.clone();
        if let Some(mut pty) = pty_pool.remove(&legacy_key) {
            let _ = pty.child.kill();
            let _ = pty.child.wait();
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
    let mut pool = state.live_processes.lock().await;
    let sdk_pool = state.sdk_sessions.lock().await;
    let process_alive = if let Some(proc) = pool.get_mut(&proc_k) {
        !matches!(proc.child.try_wait(), Ok(Some(_)))
    } else if let Some(sdk_session) = sdk_pool.get(&proc_k) {
        sdk_session.is_connected()
    } else {
        false
    };

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

    // Remove live process (Drop impl sends SIGKILL)
    {
        let mut pool = state.live_processes.lock().await;
        let proc_k = process_key(&id, &session_id);
        pool.remove(&proc_k);
    }

    // Disconnect SDK session if present
    {
        let mut sdk_pool = state.sdk_sessions.lock().await;
        let proc_k = process_key(&id, &session_id);
        if let Some(mut session) = sdk_pool.remove(&proc_k) {
            if let Err(e) = session.disconnect().await {
                tracing::warn!(error = %e, "failed to disconnect SDK session on kill");
            }
        }
    }

    // Kill PTY process
    {
        let mut pty_pool = state.pty_processes.lock().await;
        let pty_k = super::terminal::pty_key(&id, Some(&session_id));
        if let Some(mut pty) = pty_pool.remove(&pty_k) {
            let _ = pty.child.kill();
            let _ = pty.child.wait();
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

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_to_disk(&sessions_snapshot);

    Ok(Json(json!({ "status": "killed" })))
}

#[derive(Deserialize)]
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
    }
}

/// SDK-based chat stream. Used when AGENT_SDK_ENABLED=1.
fn chat_sdk_stream(
    state: AppState,
    id: String,
    prompt: String,
    target_session_id: String,
    is_new: bool,
    working_dir: String,
    system_prompt: Option<String>,
    agent: crate::agents::Agent,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let key_for_stream = agent_key(&id);
        let proc_key = process_key(&id, &target_session_id);
        let sessions_ref = state.interact_sessions.clone();
        let sessions_path = state.sessions_path.clone();
        let sdk_sessions = state.sdk_sessions.clone();
        let session_streams = state.session_streams.clone();
        let chat_event_buffers = state.chat_event_buffers.clone();
        let data_dir = state.data_dir.clone();

        // Create or get the SDK session
        let needs_create = {
            let pool = sdk_sessions.lock().await;
            !pool.contains_key(&proc_key) || !pool.get(&proc_key).map_or(false, |s| s.is_connected())
        };

        if needs_create {
            let config = build_sdk_config(
                &agent,
                &target_session_id,
                &working_dir,
                is_new,
                system_prompt.as_deref(),
            );

            match crate::agent_sdk::AgentSession::create(
                config,
                &id,
                &target_session_id,
            ).await {
                Ok(session) => {
                    let mut pool = sdk_sessions.lock().await;
                    // Remove old disconnected session if present
                    pool.remove(&proc_key);
                    pool.insert(proc_key.clone(), session);

                    yield Ok(Event::default().event("system").data(
                        serde_json::to_string(&json!({"message": "Session started (SDK)"})).unwrap()
                    ));
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to create AgentSession");
                    let mut all_sessions = sessions_ref.write().await;
                    if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                        if let Some(s) = fs.get_session_mut(&target_session_id) {
                            s.busy = false;
                            s.busy_since = None;
                        }
                    }
                    yield Ok(Event::default().event("error").data(
                        serde_json::to_string(&json!({"message": format!("failed to create SDK session: {e}")})).unwrap()
                    ));
                    return;
                }
            }
        } else {
            yield Ok(Event::default().event("system").data(
                serde_json::to_string(&json!({"message": "Ready (SDK)"})).unwrap()
            ));
        }

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

        {
            let bc_tx = bc_tx.clone();
            let sdk_sessions = sdk_sessions.clone();
            let sessions_ref = sessions_ref.clone();
            let session_streams = session_streams.clone();
            let chat_event_buffers = chat_event_buffers.clone();
            let proc_key = proc_key.clone();
            let key_for_bg = key_for_stream.clone();
            let sid_for_bg = target_session_id.clone();
            let sessions_path = sessions_path.clone();
            let log_path = log_path.clone();

            tokio::spawn(async move {
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
                let result = {
                    let mut pool = sdk_sessions.lock().await;
                    if let Some(session) = pool.get_mut(&proc_key) {
                        match session.send_message(&prompt).await {
                            Ok(mut rx) => {
                                drop(pool);
                                loop {
                                    match rx.recv().await {
                                        Ok(sse_event) => {
                                            let event_str = format!("{}:{}", sse_event.event_type,
                                                serde_json::to_string(&sse_event.data).unwrap_or_default());

                                            if sse_event.event_type == "result" {
                                                session_cost = sse_event.data.get("cost")
                                                    .and_then(|v| v.as_f64())
                                                    .unwrap_or(0.0);
                                            }

                                            let _ = bc_tx.send(event_str.clone());
                                            append_log(&event_str);
                                            {
                                                let mut buffers = chat_event_buffers.lock().await;
                                                if let Some(buf) = buffers.get_mut(&proc_key) {
                                                    buf.push(event_str);
                                                }
                                            }

                                            if sse_event.event_type == "done" {
                                                break;
                                            }
                                        }
                                        Err(broadcast::error::RecvError::Lagged(n)) => {
                                            tracing::warn!(skipped = n, "SDK event receiver lagged");
                                        }
                                        Err(broadcast::error::RecvError::Closed) => {
                                            break;
                                        }
                                    }
                                }
                                Ok(())
                            }
                            Err(e) => {
                                drop(pool);
                                Err(e)
                            }
                        }
                    } else {
                        Err(anyhow::anyhow!("SDK session not found in pool"))
                    }
                };

                if let Err(e) = result {
                    tracing::error!(error = %e, "SDK send_message failed");
                    let error_event = format!("error:{}", serde_json::to_string(&json!({"message": e.to_string()})).unwrap_or_default());
                    let _ = bc_tx.send(error_event.clone());
                    append_log(&error_event);
                    {
                        let mut buffers = chat_event_buffers.lock().await;
                        if let Some(buf) = buffers.get_mut(&proc_key) {
                            buf.push(error_event);
                        }
                    }
                }

                // Send git snapshot after completion
                {
                    let sessions = sessions_ref.read().await;
                    if let Some(fs) = sessions.get(&key_for_bg) {
                        if let Some(s) = fs.get_session(&sid_for_bg) {
                            if let Some(ref wt_meta) = s.worktree_group {
                                let snapshot = crate::git::snapshot_from_meta(wt_meta);
                                if let Ok(snap_json) = serde_json::to_string(&snapshot) {
                                    let git_event = format!("git_snapshot:{snap_json}");
                                    let _ = bc_tx.send(git_event.clone());
                                    append_log(&git_event);
                                    let mut buffers = chat_event_buffers.lock().await;
                                    if let Some(buf) = buffers.get_mut(&proc_key) {
                                        buf.push(git_event);
                                    }
                                }
                            }
                        }
                    }
                }

                // Send done event
                let done_data = serde_json::to_string(&json!({"exit_code": 0})).unwrap();
                let done_event = format!("done:{done_data}");
                let _ = bc_tx.send(done_event.clone());
                append_log(&done_event);
                {
                    let mut buffers = chat_event_buffers.lock().await;
                    if let Some(buf) = buffers.get_mut(&proc_key) {
                        buf.push(done_event);
                    }
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
                    let sessions_snapshot = all_sessions.clone();
                    drop(all_sessions);
                    crate::api::save_sessions(&sessions_path, &sessions_snapshot);
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

    let permissions = agent.permissions.clone();
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

    // Look up or create the session
    let (target_session_id, is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;

        let flow_sessions = all_sessions
            .entry(key.clone())
            .or_insert_with(|| {
                let sid = Uuid::new_v4().to_string();
                FlowSessions {
                    flow_name: agent.name.clone(),
                    active_session: sid.clone(),
                    sessions: vec![InteractSession {
                        session_id: sid,
                        summary: make_summary(&prompt),
                        node_id: None,
                        working_dir: default_working_dir.clone(),
                        active_pid: None,
                        busy: false,
                        busy_since: None,
                        message_count: 0,
                        total_cost: 0.0,
                        created_at: Utc::now().to_rfc3339(),
                        skills_dir: None,
                        kind: "interactive".to_string(),
                        flow_run: None,
                        worktree_group: None,
                    }],
                }
            });

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
            // Check for stale busy — process might be dead
            let proc_k = process_key(&id, &session.session_id);
            let is_stale = {
                let mut pool = state.live_processes.lock().await;
                if let Some(proc) = pool.get_mut(&proc_k) {
                    // Process exists — check if it's actually dead
                    matches!(proc.child.try_wait(), Ok(Some(_)))
                } else {
                    // Check SDK sessions
                    let sdk_pool = state.sdk_sessions.lock().await;
                    if let Some(sdk_session) = sdk_pool.get(&proc_k) {
                        // SDK session exists — it's stale if disconnected
                        !sdk_session.is_connected()
                    } else {
                        // No process in any pool — check if it's been busy too long
                        session.busy_since
                            .map(|since| chrono::Utc::now().signed_duration_since(since).to_std().unwrap_or_default() > STALE_BUSY_TIMEOUT)
                            .unwrap_or(true) // No timestamp = definitely stale
                    }
                }
            };

            if is_stale {
                tracing::warn!(
                    session_id = %session.session_id,
                    "auto-recovering stale busy session"
                );
                // Clean up dead process
                let proc_k = process_key(&id, &session.session_id);
                let mut pool = state.live_processes.lock().await;
                pool.remove(&proc_k);
                drop(pool);
                // Also clean up SDK session
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

        let sessions_snapshot = all_sessions.clone();
        drop(all_sessions);
        state.save_sessions_to_disk(&sessions_snapshot);

        (sid, is_new, wdir)
    };

    // On first message: generate .skills/ context files and build system prompt
    let data_dir = state.data_dir.clone();

    // Generate .claude/settings.local.json with hook URLs for this session
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

        // Hook config: 3 levels — event -> matcher group -> hook handlers
        // See https://code.claude.com/docs/en/hooks
        //
        // NOTE: PermissionRequest hooks do NOT fire in stream-json mode.
        // We use PreToolUse as the permission gate instead — it fires for every
        // tool call and can return permissionDecision: "allow" or "deny".
        settings["hooks"] = json!({
            "PreToolUse": [{
                "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
                "hooks": [{
                    "type": "http",
                    "url": format!("{hook_base}/pre-tool-use?session_id={sid}"),
                    "timeout": 120
                }]
            }],
            "PostToolUse": [{
                "matcher": "Write|Edit|MultiEdit|NotebookEdit|Bash",
                "hooks": [{
                    "type": "http",
                    "url": format!("{hook_base}/post-tool-use?session_id={sid}")
                }]
            }],
            "Stop": [{
                "hooks": [{
                    "type": "http",
                    "url": format!("{hook_base}/stop?session_id={sid}")
                }]
            }]
        });

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
                let sessions_snapshot = all_sessions.clone();
                drop(all_sessions);
                state.save_sessions_to_disk(&sessions_snapshot);
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

    // -----------------------------------------------------------------------
    // SDK path: when AGENT_SDK_ENABLED=1, use AgentSession instead of
    // LiveClaudeProcess. Returns early with the SDK stream.
    // -----------------------------------------------------------------------
    if agent_sdk_enabled() {
        let stream = chat_sdk_stream(
            state,
            id,
            prompt,
            target_session_id,
            is_new,
            working_dir,
            system_prompt,
            agent,
        );
        let boxed: BoxSseStream = Box::pin(stream);
        return Ok(Sse::new(boxed).keep_alive(
            KeepAlive::new().interval(std::time::Duration::from_secs(15)),
        ));
    }

    // -----------------------------------------------------------------------
    // Legacy path: LiveClaudeProcess (stdin/stdout subprocess)
    // -----------------------------------------------------------------------

    let key_for_stream = key.clone();
    let proc_key_for_stream = process_key(&id, &target_session_id);
    let session_id_for_stream = target_session_id.clone();
    let sessions_ref = state.interact_sessions.clone();
    let sessions_path = state.sessions_path.clone();
    let live_processes = state.live_processes.clone();
    let session_streams = state.session_streams.clone();
    let chat_event_buffers = state.chat_event_buffers.clone();

    let stream = async_stream::stream! {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        // Check if we have a live persistent process for this session.
        // Hold the lock across check + spawn + insert to prevent TOCTOU races.
        let spawn_result = {
            let mut pool = live_processes.lock().await;
            if pool.contains_key(&proc_key_for_stream) {
                None // Already exists, no spawn needed
            } else {
                // Spawn inside the lock — Command::spawn() is synchronous (forks immediately)
                let mut args = vec![
                    "--verbose".to_string(),
                    "--output-format".to_string(),
                    "stream-json".to_string(),
                    "--input-format".to_string(),
                    "stream-json".to_string(),
                ];

                if !permissions.is_empty() {
                    // Agent has explicit allowedTools — use them.
                    // Tools NOT in this list trigger Claude's permission model,
                    // which fires the PermissionRequest hook.
                    args.push("--allowedTools".to_string());
                    args.push(permissions.join(","));
                }
                // When no explicit permissions: Claude's default permission model
                // kicks in and fires PermissionRequest hooks (configured in
                // .claude/settings.local.json) for tools that need approval.

                if is_new {
                    args.push("--session-id".to_string());
                    args.push(session_id_for_stream.clone());
                    if let Some(ref sys_prompt) = system_prompt {
                        args.push("--system-prompt".to_string());
                        args.push(sys_prompt.clone());
                    }
                } else {
                    args.push("--resume".to_string());
                    args.push(session_id_for_stream.clone());
                }

                tracing::info!(
                    key = %proc_key_for_stream,
                    session_id = %session_id_for_stream,
                    is_new,
                    "spawning persistent claude for agent chat"
                );

                match Command::new("claude")
                    .args(&args)
                    .current_dir(&working_dir)
                    .env_remove("CLAUDECODE")
                    .env("CLAUDECODE", "")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(mut child) => {
                        if let Some(pid) = child.id() {
                            let mut all_sessions = sessions_ref.write().await;
                            if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                                if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                                    s.active_pid = Some(pid);
                                }
                            }
                        }

                        let child_stdin = child.stdin.take().expect("stdin piped");

                        let stdout = child.stdout.take().expect("stdout piped");
                        let (stdout_tx, stdout_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                        tokio::spawn(async move {
                            let reader = BufReader::new(stdout);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if stdout_tx.send(line).is_err() {
                                    break;
                                }
                            }
                        });

                        let stderr = child.stderr.take().expect("stderr piped");
                        let (stderr_tx, stderr_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                        tokio::spawn(async move {
                            let reader = BufReader::new(stderr);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if !line.is_empty() {
                                    let _ = stderr_tx.send(line);
                                }
                            }
                        });

                        let live_proc = LiveClaudeProcess {
                            stdin: child_stdin,
                            stdout_lines: stdout_rx,
                            stderr_lines: stderr_rx,
                            child,
                            busy: false,
                        };

                        pool.insert(proc_key_for_stream.clone(), live_proc);
                        Some(Ok(()))
                    }
                    Err(e) => Some(Err(e)),
                }
            }
        };

        match spawn_result {
            Some(Err(e)) => {
                tracing::error!(error = %e, "failed to spawn claude for agent chat");
                let mut all_sessions = sessions_ref.write().await;
                if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                    if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                        s.busy = false;
                        s.busy_since = None;
                    }
                }
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("failed to spawn claude: {e}")})).unwrap()
                ));
                return;
            }
            Some(Ok(())) => {
                yield Ok(Event::default().event("system").data(
                    serde_json::to_string(&json!({"message": "Session started"})).unwrap()
                ));
            }
            None => {
                yield Ok(Event::default().event("system").data(
                    serde_json::to_string(&json!({"message": "Ready"})).unwrap()
                ));
            }
        }

        // Write the prompt to the persistent process's stdin as stream-json
        {
            let mut pool = live_processes.lock().await;
            if let Some(proc) = pool.get_mut(&proc_key_for_stream) {
                // Build content: plain string when no images, content block array when images present
                let content = if images.is_empty() {
                    json!(prompt)
                } else {
                    let mut blocks: Vec<Value> = Vec::new();
                    if !prompt.trim().is_empty() {
                        blocks.push(json!({ "type": "text", "text": prompt }));
                    }
                    for img in &images {
                        blocks.push(json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": img.media_type,
                                "data": img.data,
                            }
                        }));
                    }
                    json!(blocks)
                };
                let input_msg = serde_json::to_string(&json!({
                    "type": "user",
                    "message": {
                        "role": "user",
                        "content": content,
                    }
                })).unwrap();
                let write_result = proc.stdin.write_all(format!("{input_msg}\n").as_bytes()).await;
                if let Err(e) = write_result {
                    tracing::error!(error = %e, "failed to write to persistent claude stdin");
                    pool.remove(&proc_key_for_stream);
                    let mut all_sessions = sessions_ref.write().await;
                    if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                        if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                            s.busy = false;
                            s.busy_since = None;
                            s.active_pid = None;
                        }
                    }
                    yield Ok(Event::default().event("error").data(
                        serde_json::to_string(&json!({"message": format!("stdin write failed: {e}. Session will restart on next message.")})).unwrap()
                    ));
                    return;
                }
                proc.busy = true;
            } else {
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": "process not found in pool"})).unwrap()
                ));
                return;
            }
        }

        // Create broadcast channel + event buffer, then spawn background reader task.
        let (bc_tx, _) = broadcast::channel::<String>(1024);
        {
            let mut streams = session_streams.lock().await;
            streams.insert(proc_key_for_stream.clone(), bc_tx.clone());
            tracing::info!(
                proc_key = %proc_key_for_stream,
                total_streams = streams.len(),
                "[RECONNECT-DEBUG] Created broadcast channel and inserted into session_streams"
            );
        }
        {
            let mut buffers = chat_event_buffers.lock().await;
            buffers.insert(proc_key_for_stream.clone(), Vec::new());
            tracing::info!(
                proc_key = %proc_key_for_stream,
                "[RECONNECT-DEBUG] Created event buffer"
            );
        }

        // Spawn the background reader task
        {
            let bc_tx = bc_tx.clone();
            let live_processes = live_processes.clone();
            let sessions_ref = sessions_ref.clone();
            let session_streams = session_streams.clone();
            let chat_event_buffers = chat_event_buffers.clone();
            let proc_key = proc_key_for_stream.clone();
            let key_for_bg = key_for_stream.clone();
            let sid_for_bg = session_id_for_stream.clone();
            let sessions_path = sessions_path.clone();
            let data_dir_for_bg = data_dir.clone();
            let prompt_for_log = prompt.clone();

            tokio::spawn(async move {
                tracing::info!(
                    proc_key = %proc_key,
                    "[RECONNECT-DEBUG] Background reader task STARTED"
                );
                let mut session_cost: f64 = 0.0;
                let mut event_count: u64 = 0;

                // Set up JSONL log file for session history persistence
                let logs_dir = data_dir_for_bg.join("session_logs");
                let _ = std::fs::create_dir_all(&logs_dir);
                let log_path = logs_dir.join(format!("{sid_for_bg}.jsonl"));

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

                // Log the user prompt that initiated this turn
                append_log(&format!("user:{}", serde_json::to_string(&json!({"text": prompt_for_log})).unwrap_or_default()));

                loop {
                    let (line, stderr_batch) = {
                        let mut pool = live_processes.lock().await;
                        if let Some(proc) = pool.get_mut(&proc_key) {
                            let mut errs = Vec::new();
                            while let Ok(err_line) = proc.stderr_lines.try_recv() {
                                errs.push(err_line);
                            }
                            let stdout_line = proc.stdout_lines.try_recv().ok();
                            (stdout_line, errs)
                        } else {
                            break;
                        }
                    };

                    for err_line in stderr_batch {
                        tracing::debug!(stderr = %err_line, "claude stderr");
                        let event = format!("stderr:{err_line}");
                        let _ = bc_tx.send(event.clone());
                        append_log(&event);
                        let mut buffers = chat_event_buffers.lock().await;
                        if let Some(buf) = buffers.get_mut(&proc_key) {
                            buf.push(event);
                        }
                    }

                    if let Some(line) = line {
                        if line.is_empty() {
                            continue;
                        }

                        let events = parse_claude_line_to_sse_events(&line);
                        let mut is_result = false;

                        for (event_type, data_json) in &events {
                            if event_type == "result" {
                                is_result = true;
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_json) {
                                    session_cost = val.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                }
                            }
                            let event_str = format!("{event_type}:{data_json}");
                            event_count += 1;
                            let receivers = bc_tx.send(event_str.clone());
                            tracing::debug!(
                                proc_key = %proc_key,
                                event_type = %event_type,
                                event_count,
                                receivers = ?receivers,
                                "[RECONNECT-DEBUG] Background task broadcast event"
                            );
                            append_log(&event_str);
                            let mut buffers = chat_event_buffers.lock().await;
                            if let Some(buf) = buffers.get_mut(&proc_key) {
                                buf.push(event_str);
                            }
                        }

                        if is_result {
                            // Send git snapshot after result (before done)
                            {
                                let sessions = sessions_ref.read().await;
                                if let Some(fs) = sessions.get(&key_for_bg) {
                                    if let Some(s) = fs.get_session(&sid_for_bg) {
                                        if let Some(ref wt_meta) = s.worktree_group {
                                            let snapshot = crate::git::snapshot_from_meta(wt_meta);
                                            if let Ok(snap_json) = serde_json::to_string(&snapshot) {
                                                let git_event = format!("git_snapshot:{snap_json}");
                                                let _ = bc_tx.send(git_event.clone());
                                                append_log(&git_event);
                                                let mut buffers = chat_event_buffers.lock().await;
                                                if let Some(buf) = buffers.get_mut(&proc_key) {
                                                    buf.push(git_event);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

                        let mut pool = live_processes.lock().await;
                        if let Some(proc) = pool.get_mut(&proc_key) {
                            if let Ok(Some(_status)) = proc.child.try_wait() {
                                pool.remove(&proc_key);
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }

                // Mark session as not busy, update stats
                {
                    let mut pool = live_processes.lock().await;
                    if let Some(proc) = pool.get_mut(&proc_key) {
                        proc.busy = false;
                    }
                }
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
                    let sessions_snapshot = all_sessions.clone();
                    drop(all_sessions);
                    crate::api::save_sessions(&sessions_path, &sessions_snapshot);
                }

                // Send done event, then clean up broadcast/buffer
                tracing::info!(
                    proc_key = %proc_key,
                    event_count,
                    "[RECONNECT-DEBUG] Background reader task DONE, sending done event"
                );
                let done_data = serde_json::to_string(&json!({"exit_code": 0})).unwrap();
                let done_event = format!("done:{done_data}");
                let _ = bc_tx.send(done_event.clone());
                append_log(&done_event);
                {
                    let mut buffers = chat_event_buffers.lock().await;
                    if let Some(buf) = buffers.get_mut(&proc_key) {
                        buf.push(done_event);
                    }
                }

                // Clean up broadcast channel after a delay to allow reconnects to catch the done event
                tracing::info!(
                    proc_key = %proc_key,
                    "[RECONNECT-DEBUG] Waiting 5s before cleanup..."
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                {
                    let mut streams = session_streams.lock().await;
                    streams.remove(&proc_key);
                    tracing::info!(
                        proc_key = %proc_key,
                        remaining_streams = streams.len(),
                        "[RECONNECT-DEBUG] Cleaned up broadcast channel"
                    );
                }
                {
                    let mut buffers = chat_event_buffers.lock().await;
                    buffers.remove(&proc_key);
                    tracing::info!(
                        proc_key = %proc_key,
                        "[RECONNECT-DEBUG] Cleaned up event buffer. Background task EXIT."
                    );
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
                    tracing::warn!(skipped = n, "agent chat broadcast subscriber lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    let boxed: BoxSseStream = Box::pin(stream);
    Ok(Sse::new(boxed).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

/// Parse a raw Claude stdout line into a list of (event_type, data_json) pairs
/// for broadcasting. Same parsing logic as the old inline read loop.
fn parse_claude_line_to_sse_events(line: &str) -> Vec<(String, String)> {
    let mut events = Vec::new();

    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(line) {
        let event_type = json_val.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

        match event_type {
            "system" => {
                // Skip system events on resume
            }
            "content_block_delta" => {
                if let Some(delta) = json_val.get("delta") {
                    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if delta_type == "text_delta" {
                        let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            events.push((
                                "text".to_string(),
                                serde_json::to_string(&json!({"text": text})).unwrap(),
                            ));
                        }
                    }
                }
            }
            "content_block_start" => {
                if let Some(content_block) = json_val.get("content_block") {
                    let block_type = content_block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if block_type == "tool_use" {
                        let tool = content_block.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        events.push((
                            "tool_use".to_string(),
                            serde_json::to_string(&json!({"tool": tool, "input": ""})).unwrap(),
                        ));
                    }
                }
            }
            "assistant" => {
                if let Some(content) = json_val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                    for block in content {
                        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match block_type {
                            "tool_use" => {
                                let tool = block.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let input = block.get("input").map(|v| {
                                    if v.is_string() {
                                        v.as_str().unwrap_or("").to_string()
                                    } else {
                                        serde_json::to_string(v).unwrap_or_default()
                                    }
                                }).unwrap_or_default();
                                events.push((
                                    "tool_use".to_string(),
                                    serde_json::to_string(&json!({"tool": tool, "input": input})).unwrap(),
                                ));
                            }
                            "tool_result" => {
                                let result_content = block.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                events.push((
                                    "tool_result".to_string(),
                                    serde_json::to_string(&json!({"content": result_content})).unwrap(),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                let cost = json_val.get("total_cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let turns = json_val.get("num_turns").and_then(|v| v.as_u64()).unwrap_or(0);
                let result_text = json_val.get("result").and_then(|v| v.as_str()).unwrap_or("");
                events.push((
                    "result".to_string(),
                    serde_json::to_string(&json!({"text": result_text, "cost": cost, "turns": turns})).unwrap(),
                ));
            }
            _ => {}
        }
    } else {
        events.push((
            "text".to_string(),
            serde_json::to_string(&json!({"text": line})).unwrap(),
        ));
    }

    events
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

// ---------------------------------------------------------------------------
// Persistent SSE endpoint for hook events (permission requests, file changes, stop)
// ---------------------------------------------------------------------------

/// GET /agents/{id}/sessions/{session_id}/hooks/stream
///
/// Long-lived SSE connection that delivers hook events for a session.
/// Unlike chat streams, this stays open as long as the frontend is subscribed.
pub async fn hook_event_stream(
    State(state): State<AppState>,
    Path((agent_id, session_id)): Path<(String, String)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let hook_key = format!("agent::{agent_id}::session::{session_id}");

    // Get or create a broadcast channel for this session's hook events
    let rx = {
        let mut streams = state.hook_streams.lock().await;
        let tx = streams
            .entry(hook_key.clone())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel::<String>(256);
                tx
            });
        tx.subscribe()
    };

    tracing::info!(
        hook_key = %hook_key,
        "hook event stream connected"
    );

    let hook_streams = state.hook_streams.clone();
    let hook_key_cleanup = hook_key.clone();

    let stream = async_stream::stream! {
        let mut rx = rx;

        // Send an initial keepalive/connected event
        yield Ok(Event::default().event("connected").data(
            serde_json::to_string(&json!({"session_id": session_id})).unwrap_or_default()
        ));

        loop {
            match rx.recv().await {
                Ok(data) => {
                    // Parse to extract event type, or send as generic hook event
                    if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
                        let event_type = parsed.get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("hook");
                        yield Ok(Event::default().event(event_type).data(data));
                    } else {
                        yield Ok(Event::default().event("hook").data(data));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(hook_key = %hook_key_cleanup, lagged = n, "hook stream lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!(hook_key = %hook_key_cleanup, "hook stream closed");
                    break;
                }
            }
        }

        // Cleanup: remove from hook_streams if no more subscribers
        {
            let mut streams = hook_streams.lock().await;
            if let Some(tx) = streams.get(&hook_key_cleanup) {
                if tx.receiver_count() == 0 {
                    streams.remove(&hook_key_cleanup);
                    tracing::info!(hook_key = %hook_key_cleanup, "cleaned up hook stream (no subscribers)");
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
