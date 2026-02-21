use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use futures::stream::Stream;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use uuid::Uuid;

use super::AppState;
use crate::flows::{Edge, Flow, Node};

pub fn flow_router() -> Router<AppState> {
    Router::new()
        .route("/flows", get(list_flows).post(create_flow))
        .route(
            "/flows/{id}",
            get(get_flow).put(update_flow).delete(delete_flow),
        )
        .route("/flows/{id}/trigger", post(trigger_flow))
        .route("/flows/{id}/session", get(get_session))
        .route("/flows/{id}/interact", post(interact_flow))
        .route("/flows/{id}/interact/sessions", get(list_sessions))
        .route("/flows/{id}/interact/new", post(new_session))
        .route("/flows/{id}/interact/sessions/{session_id}", delete(delete_session))
        .route("/flows/{id}/interact/reset", post(reset_interact))
        .route("/flows/{id}/interact/stop", post(stop_interact))
        .route("/flows/{id}/runs", get(get_runs))
        .route("/flows/{id}/runs/live", get(stream_runs))
        .route("/node-types", get(get_node_types))
}

async fn list_flows(State(state): State<AppState>) -> Json<Value> {
    let flows = state.store.list_flows().await;

    let summaries: Vec<Value> = flows
        .iter()
        .map(|f| {
            json!({
                "id": f.id,
                "name": f.name,
                "description": f.description,
                "enabled": f.enabled,
                "node_count": f.nodes.len(),
                "edge_count": f.edges.len(),
                "created_at": f.created_at,
                "updated_at": f.updated_at,
            })
        })
        .collect();

    Json(json!({ "flows": summaries }))
}

async fn get_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    Ok(Json(serde_json::to_value(&flow).unwrap()))
}

#[derive(Deserialize)]
struct CreateFlowRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

async fn create_flow(
    State(state): State<AppState>,
    Json(body): Json<CreateFlowRequest>,
) -> (StatusCode, Json<Value>) {
    let now = Utc::now();
    let flow = Flow {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        description: body.description,
        enabled: true,
        nodes: body.nodes,
        edges: body.edges,
        created_at: now,
        updated_at: now,
    };

    let id = flow.id.clone();
    if let Err(e) = state.store.save_flow(flow).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        );
    }

    // Start scheduler trigger for the new flow
    if let Err(e) = state.scheduler.start_flow(&id).await {
        tracing::warn!(flow_id = %id, error = %e, "Failed to start trigger for new flow");
    }

    (StatusCode::CREATED, Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct UpdateFlowRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    nodes: Option<Vec<Node>>,
    #[serde(default)]
    edges: Option<Vec<Edge>>,
}

async fn update_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFlowRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    if let Some(name) = body.name {
        flow.name = name;
    }
    if let Some(description) = body.description {
        flow.description = description;
    }
    if let Some(enabled) = body.enabled {
        flow.enabled = enabled;
    }
    if let Some(nodes) = body.nodes {
        flow.nodes = nodes;
    }
    if let Some(edges) = body.edges {
        flow.edges = edges;
    }
    flow.updated_at = Utc::now();

    state.store.save_flow(flow.clone()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        )
    })?;

    // Restart scheduler trigger (handles enable/disable/config changes)
    if let Err(e) = state.scheduler.restart_flow(&id).await {
        tracing::warn!(flow_id = %id, error = %e, "Failed to restart trigger for updated flow");
    }

    Ok(Json(serde_json::to_value(&flow).unwrap()))
}

async fn delete_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Stop scheduler trigger before deleting
    state.scheduler.stop_flow(&id).await;

    let existed = state.store.delete_flow(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to delete flow: {e}") })),
        )
    })?;

    if !existed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        ));
    }

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct TriggerFlowRequest {
    repo: Option<String>,
    pr: Option<u64>,
}

async fn trigger_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: String,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    // Check if this is a PR trigger request
    let trigger_body: Option<TriggerFlowRequest> = if body.trim().is_empty() {
        None
    } else {
        serde_json::from_str(&body).ok()
    };

    if let Some(trigger_body) = &trigger_body {
        if let (Some(repo), Some(pr)) = (&trigger_body.repo, trigger_body.pr) {
            let scheduler = state.scheduler.clone();
            let flow_id = id.clone();
            let repo = repo.clone();
            let repo_for_response = repo.clone();

            tokio::spawn(async move {
                if let Err(e) = scheduler.trigger_pr_review(&flow_id, &repo, pr).await {
                    tracing::error!(flow_id = %flow_id, repo = %repo, pr, error = %e, "Manual PR trigger failed");
                }
            });

            return Ok((
                StatusCode::ACCEPTED,
                Json(json!({ "status": "pr_review_started", "flow_id": id, "repo": repo_for_response, "pr": pr })),
            ));
        }
    }

    // Default: one-shot flow execution
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: Some(state.events_tx.clone()),
    };

    let store = state.store.clone();
    let flow_name = flow.name.clone();

    tokio::spawn(async move {
        match runner.execute(&flow, &*store).await {
            Ok(run) => {
                tracing::info!(
                    flow = %flow_name,
                    run_id = %run.id,
                    "Flow execution completed"
                );
            }
            Err(e) => {
                tracing::error!(flow = %flow_name, error = %e, "Flow execution failed");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "triggered", "flow_id": id })),
    ))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
    };

    // Fallback to minimal session if prepare_session fails (e.g. no executor node).
    // Interact should always work — even on empty flows.
    let session = match runner.prepare_session(&flow).await {
        Ok(s) => s,
        Err(_) => crate::flows::runner::SessionInfo {
            flow_id: flow.id.clone(),
            flow_name: flow.name.clone(),
            prompt: String::new(),
            permissions: vec![],
            append_system_prompt: None,
            working_dir: std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .to_string(),
            sources_summary: "No sources configured".into(),
            sinks_summary: "No sinks configured".into(),
        },
    };

    Ok(Json(serde_json::to_value(&session).unwrap()))
}

// ---------------------------------------------------------------------------
// Interact endpoints: multi-session (History / tabs)
// ---------------------------------------------------------------------------

/// Resolve working_dir from flow config or fallback to cwd.
fn resolve_working_dir(session_info: &Option<crate::flows::runner::SessionInfo>) -> String {
    session_info
        .as_ref()
        .map(|s| s.working_dir.clone())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .to_string()
        })
}

/// Truncate a string to ~80 chars for use as a summary, breaking at word boundary.
/// Uses char-safe indexing to avoid panics on multi-byte UTF-8 characters.
fn make_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    // Collect first 80 chars safely, then find a word boundary
    let truncated: String = trimmed.chars().take(80).collect();
    let boundary = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}...", &truncated[..boundary])
}

/// GET /flows/{id}/interact/sessions — list all sessions for a workflow
async fn list_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let sessions = state.interact_sessions.read().await;
    if let Some(flow_sessions) = sessions.get(&id) {
        let list: Vec<Value> = flow_sessions
            .sessions
            .iter()
            .map(|s| {
                json!({
                    "session_id": s.session_id,
                    "summary": s.summary,
                    "message_count": s.message_count,
                    "total_cost": s.total_cost,
                    "created_at": s.created_at,
                    "busy": s.busy,
                })
            })
            .collect();
        Json(json!({
            "flow_name": flow_sessions.flow_name,
            "active_session": flow_sessions.active_session,
            "sessions": list,
        }))
    } else {
        Json(json!({
            "flow_name": "",
            "active_session": "",
            "sessions": [],
        }))
    }
}

/// POST /flows/{id}/interact/new — create a new session tab
async fn new_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
    };
    let session_info = runner.prepare_session(&flow).await.ok();
    let working_dir = resolve_working_dir(&session_info);

    let new_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let mut all_sessions = state.interact_sessions.write().await;
    let flow_sessions = all_sessions
        .entry(id.clone())
        .or_insert_with(|| super::FlowSessions {
            flow_name: flow.name.clone(),
            active_session: String::new(),
            sessions: Vec::new(),
        });

    // Soft limit warning
    let warning = if flow_sessions.sessions.len() >= 10 {
        Some("Consider closing old sessions (10+ open)")
    } else {
        None
    };

    flow_sessions.sessions.push(super::InteractSession {
        session_id: new_id.clone(),
        summary: String::new(),
        working_dir,
        active_pid: None,
        busy: false,
        message_count: 0,
        total_cost: 0.0,
        created_at: now.clone(),
    });
    flow_sessions.active_session = new_id.clone();
    flow_sessions.flow_name = flow.name.clone();

    // Clone data for persistence and drop the write lock before disk I/O
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    super::save_sessions(&state.sessions_path, &sessions_snapshot);

    let mut resp = json!({ "session_id": new_id, "created_at": now });
    if let Some(w) = warning {
        resp["warning"] = json!(w);
    }
    Ok(Json(resp))
}

/// DELETE /flows/{id}/interact/sessions/{session_id} — remove a specific session
async fn delete_session(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut all_sessions = state.interact_sessions.write().await;

    let active_after = {
        let flow_sessions = all_sessions.get_mut(&id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "no sessions for this flow" })),
            )
        })?;

        if flow_sessions.sessions.len() <= 1 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "cannot delete the last session" })),
            ));
        }

        // Kill process if running
        if let Some(session) = flow_sessions.get_session(&session_id) {
            if let Some(pid) = session.active_pid {
                kill_pid(pid);
            }
        }

        flow_sessions.sessions.retain(|s| s.session_id != session_id);

        // If we deleted the active session, switch to the most recent remaining
        if flow_sessions.active_session == session_id {
            if let Some(last) = flow_sessions.sessions.last() {
                flow_sessions.active_session = last.session_id.clone();
            }
        }

        flow_sessions.active_session.clone()
    }; // flow_sessions borrow dropped here

    // Clone and drop write lock before disk I/O
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    super::save_sessions(&state.sessions_path, &sessions_snapshot);

    Ok(Json(json!({
        "deleted": true,
        "active_session": active_after,
    })))
}

#[derive(Deserialize)]
struct InteractRequest {
    prompt: String,
    session_id: Option<String>,
}

async fn interact_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<InteractRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let prompt = body.prompt;
    if prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "prompt is required" })),
        ));
    }

    // Resolve session context — try prepare_session, fallback to defaults
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
    };

    let session_info = runner.prepare_session(&flow).await.ok();

    let permissions: Vec<String> = session_info
        .as_ref()
        .map(|s| s.permissions.clone())
        .unwrap_or_default();
    let append_system_prompt = session_info
        .as_ref()
        .and_then(|s| s.append_system_prompt.clone());

    let default_working_dir = resolve_working_dir(&session_info);

    // Look up or create the session
    let (target_session_id, is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;

        let flow_sessions = all_sessions
            .entry(id.clone())
            .or_insert_with(|| {
                let sid = Uuid::new_v4().to_string();
                super::FlowSessions {
                    flow_name: flow.name.clone(),
                    active_session: sid.clone(),
                    sessions: vec![super::InteractSession {
                        session_id: sid,
                        summary: make_summary(&prompt),
                        working_dir: default_working_dir.clone(),
                        active_pid: None,
                        busy: false,
                        message_count: 0,
                        total_cost: 0.0,
                        created_at: Utc::now().to_rfc3339(),
                    }],
                }
            });

        // Keep flow_name in sync
        flow_sessions.flow_name = flow.name.clone();

        // Determine which session to target
        let target_sid = body.session_id
            .unwrap_or_else(|| flow_sessions.active_session.clone());

        // Find the target session
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
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "session is busy processing a previous message" })),
            ));
        }

        let is_new = session.message_count == 0;

        // Capture summary from first prompt
        if is_new && session.summary.is_empty() {
            session.summary = make_summary(&prompt);
        }

        session.busy = true;
        let sid = session.session_id.clone();
        let wdir = session.working_dir.clone();

        // Update active_session
        flow_sessions.active_session = sid.clone();

        // Clone and drop write lock before disk I/O
        let sessions_snapshot = all_sessions.clone();
        drop(all_sessions);
        super::save_sessions(&state.sessions_path, &sessions_snapshot);

        (sid, is_new, wdir)
    };

    // Build a system prompt for the lead agent on first message
    let system_prompt = if is_new {
        let flow_context = format!(
            "You are the lead agent for the Cthulu workflow \"{}\".\n\
             Flow description: {}\n\
             Sources: {}\n\
             Sinks: {}",
            flow.name,
            flow.description,
            session_info.as_ref().map(|s| s.sources_summary.as_str()).unwrap_or("No sources configured"),
            session_info.as_ref().map(|s| s.sinks_summary.as_str()).unwrap_or("No sinks configured"),
        );
        let base = append_system_prompt.unwrap_or_default();
        if base.is_empty() {
            Some(flow_context)
        } else {
            Some(format!("{flow_context}\n\n{base}"))
        }
    } else {
        None
    };

    let flow_id_for_stream = id.clone();
    let session_id_for_stream = target_session_id.clone();
    let sessions_ref = state.interact_sessions.clone();
    let sessions_path = state.sessions_path.clone();

    let stream = async_stream::stream! {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        let mut args = vec![
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if permissions.is_empty() {
            args.push("--dangerously-skip-permissions".to_string());
        } else {
            args.push("--allowedTools".to_string());
            args.push(permissions.join(","));
        }

        // Session persistence: --session-id for new, --resume for existing
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

        args.push("-".to_string()); // read prompt from stdin

        tracing::info!(
            flow_id = %flow_id_for_stream,
            session_id = %session_id_for_stream,
            is_new,
            "spawning claude for interact"
        );

        let mut child = match Command::new("claude")
            .args(&args)
            .current_dir(&working_dir)
            .env_remove("CLAUDECODE")
            .env("CLAUDECODE", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn claude for interact");
                let mut all_sessions = sessions_ref.write().await;
                if let Some(fs) = all_sessions.get_mut(&flow_id_for_stream) {
                    if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                        s.busy = false;
                    }
                }
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("failed to spawn claude: {e}")})).unwrap()
                ));
                return;
            }
        };

        // Store PID for potential stop
        if let Some(pid) = child.id() {
            let mut all_sessions = sessions_ref.write().await;
            if let Some(fs) = all_sessions.get_mut(&flow_id_for_stream) {
                if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                    s.active_pid = Some(pid);
                }
            }
        }

        // Write prompt to stdin
        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
                tracing::error!(error = %e, "failed to write prompt to stdin");
                let mut all_sessions = sessions_ref.write().await;
                if let Some(fs) = all_sessions.get_mut(&flow_id_for_stream) {
                    if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                        s.busy = false;
                        s.active_pid = None;
                    }
                }
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("stdin write failed: {e}")})).unwrap()
                ));
                let _ = child.kill().await;
                return;
            }
            drop(stdin);
        }

        // Stream stderr in background
        let stderr = child.stderr.take().expect("stderr piped");
        let (stderr_tx, mut stderr_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    let _ = stderr_tx.send(line);
                }
            }
        });

        // Stream stdout (stream-json events)
        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut session_cost: f64 = 0.0;

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }

            // Drain any stderr that came in
            while let Ok(err_line) = stderr_rx.try_recv() {
                yield Ok(Event::default().event("stderr").data(err_line));
            }

            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&line) {
                let event_type = json_val.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                match event_type {
                    "system" => {
                        yield Ok(Event::default().event("system").data(
                            serde_json::to_string(&json!({"message": "Session initialized"})).unwrap()
                        ));
                    }
                    "assistant" => {
                        if let Some(content) = json_val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                            for block in content {
                                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                match block_type {
                                    "text" => {
                                        let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                        if !text.is_empty() {
                                            yield Ok(Event::default().event("text").data(
                                                serde_json::to_string(&json!({"text": text})).unwrap()
                                            ));
                                        }
                                    }
                                    "tool_use" => {
                                        let tool = block.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                        let input = block.get("input").map(|v| {
                                            if v.is_string() {
                                                v.as_str().unwrap_or("").to_string()
                                            } else {
                                                serde_json::to_string(v).unwrap_or_default()
                                            }
                                        }).unwrap_or_default();
                                        yield Ok(Event::default().event("tool_use").data(
                                            serde_json::to_string(&json!({"tool": tool, "input": input})).unwrap()
                                        ));
                                    }
                                    "tool_result" => {
                                        let result_content = block.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                        yield Ok(Event::default().event("tool_result").data(
                                            serde_json::to_string(&json!({"content": result_content})).unwrap()
                                        ));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "result" => {
                        session_cost = json_val.get("total_cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let turns = json_val.get("num_turns").and_then(|v| v.as_u64()).unwrap_or(0);
                        let result_text = json_val.get("result").and_then(|v| v.as_str()).unwrap_or("");
                        yield Ok(Event::default().event("result").data(
                            serde_json::to_string(&json!({"text": result_text, "cost": session_cost, "turns": turns})).unwrap()
                        ));
                    }
                    _ => {}
                }
            } else {
                // Not JSON — raw text line
                yield Ok(Event::default().event("text").data(
                    serde_json::to_string(&json!({"text": line})).unwrap()
                ));
            }
        }

        // Drain remaining stderr
        while let Ok(err_line) = stderr_rx.try_recv() {
            yield Ok(Event::default().event("stderr").data(err_line));
        }

        // Wait for process, then update session state
        let exit_result = child.wait().await;

        {
            let mut all_sessions = sessions_ref.write().await;
            if let Some(fs) = all_sessions.get_mut(&flow_id_for_stream) {
                if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                    s.busy = false;
                    s.active_pid = None;
                    s.message_count += 1;
                    s.total_cost += session_cost;
                }
            }
            // Clone and drop write lock before disk I/O
            let sessions_snapshot = all_sessions.clone();
            drop(all_sessions);
            super::save_sessions(&sessions_path, &sessions_snapshot);
        }

        match exit_result {
            Ok(status) => {
                yield Ok(Event::default().event("done").data(
                    serde_json::to_string(&json!({"exit_code": status.code().unwrap_or(-1)})).unwrap()
                ));
            }
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("error waiting for process: {e}")})).unwrap()
                ));
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

fn kill_pid(pid: u32) {
    // Best-effort process termination, platform-specific
    #[cfg(unix)]
    {
        // Send SIGTERM via the kill binary
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

/// POST /flows/{id}/interact/reset — clear ALL sessions, create one fresh session
async fn reset_interact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&id) {
        // Kill all active processes
        for session in &flow_sessions.sessions {
            if let Some(pid) = session.active_pid {
                kill_pid(pid);
            }
        }
        // Clear all sessions
        flow_sessions.sessions.clear();
        flow_sessions.active_session = String::new();
    }
    // Remove the entry entirely (next interact will create fresh)
    all_sessions.remove(&id);
    // Clone and drop write lock before disk I/O
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    super::save_sessions(&state.sessions_path, &sessions_snapshot);
    Ok(Json(json!({ "status": "reset" })))
}

#[derive(Deserialize)]
struct StopRequest {
    session_id: Option<String>,
}

/// POST /flows/{id}/interact/stop — stop the active (or specified) session's process
async fn stop_interact(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<StopRequest>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&id) {
        let target_sid = body
            .and_then(|b| b.session_id.clone())
            .unwrap_or_else(|| flow_sessions.active_session.clone());

        if let Some(session) = flow_sessions.get_session_mut(&target_sid) {
            if let Some(pid) = session.active_pid.take() {
                kill_pid(pid);
            }
            session.busy = false;
        }
    }
    Ok(Json(json!({ "status": "stopped" })))
}

async fn get_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let runs = state.store.get_runs(&id, 100).await;
    Json(json!({ "runs": runs }))
}

async fn get_node_types() -> Json<Value> {
    Json(json!({
        "node_types": [
            {
                "kind": "cron",
                "node_type": "trigger",
                "label": "Cron Schedule",
                "config_schema": {
                    "schedule": { "type": "string", "description": "Cron expression (5-field)", "required": true },
                    "working_dir": { "type": "string", "description": "Working directory", "default": "." }
                }
            },
            {
                "kind": "github-pr",
                "node_type": "trigger",
                "label": "GitHub PR",
                "config_schema": {
                    "repos": { "type": "array", "description": "Repository configs [{slug, path}]", "required": true },
                    "poll_interval": { "type": "number", "description": "Poll interval in seconds", "default": 60 },
                    "skip_drafts": { "type": "boolean", "default": true },
                    "review_on_push": { "type": "boolean", "default": false },
                    "max_diff_size": { "type": "number", "description": "Max inline diff size in bytes", "default": 50000 }
                }
            },
            {
                "kind": "webhook",
                "node_type": "trigger",
                "label": "Webhook",
                "config_schema": {
                    "path": { "type": "string", "description": "Webhook URL path", "required": true }
                }
            },
            {
                "kind": "manual",
                "node_type": "trigger",
                "label": "Manual Trigger",
                "config_schema": {}
            },
            {
                "kind": "rss",
                "node_type": "source",
                "label": "RSS Feed",
                "config_schema": {
                    "url": { "type": "string", "description": "Feed URL", "required": true },
                    "limit": { "type": "number", "description": "Max items to fetch", "default": 10 },
                    "keywords": { "type": "array", "description": "Filter items by keywords (case-insensitive, any match)", "default": [] }
                }
            },
            {
                "kind": "web-scrape",
                "node_type": "source",
                "label": "Web Scrape",
                "config_schema": {
                    "url": { "type": "string", "description": "Page URL to scrape", "required": true },
                    "keywords": { "type": "array", "description": "Filter by keywords (case-insensitive, any match)", "default": [] }
                }
            },
            {
                "kind": "github-merged-prs",
                "node_type": "source",
                "label": "GitHub Merged PRs",
                "config_schema": {
                    "repos": { "type": "array", "description": "Repository slugs [\"owner/repo\"]", "required": true },
                    "since_days": { "type": "number", "description": "Days to look back", "default": 7 }
                }
            },
            {
                "kind": "web-scraper",
                "node_type": "source",
                "label": "Web Scraper (CSS)",
                "config_schema": {
                    "url": { "type": "string", "description": "Page URL to scrape", "required": true },
                    "base_url": { "type": "string", "description": "Base URL for resolving relative links" },
                    "items_selector": { "type": "string", "description": "CSS selector for item containers", "required": true },
                    "title_selector": { "type": "string", "description": "CSS selector for title within item" },
                    "url_selector": { "type": "string", "description": "CSS selector for link within item" },
                    "summary_selector": { "type": "string", "description": "CSS selector for summary within item" },
                    "date_selector": { "type": "string", "description": "CSS selector for date within item" },
                    "date_format": { "type": "string", "description": "Date format string (e.g. %Y-%m-%d)" },
                    "limit": { "type": "number", "description": "Max items to extract", "default": 10 }
                }
            },
            {
                "kind": "market-data",
                "node_type": "source",
                "label": "Market Data",
                "config_schema": {}
            },
            {
                "kind": "keyword",
                "node_type": "filter",
                "label": "Keyword Filter",
                "config_schema": {
                    "keywords": { "type": "array", "description": "Keywords to match (case-insensitive)", "required": true },
                    "require_all": { "type": "boolean", "description": "Require all keywords to match", "default": false },
                    "field": { "type": "string", "description": "Field to match: title, summary, or title_or_summary", "default": "title_or_summary" }
                }
            },
            {
                "kind": "claude-code",
                "node_type": "executor",
                "label": "Claude Code",
                "config_schema": {
                    "prompt": { "type": "string", "description": "Prompt file path or inline prompt", "required": true },
                    "permissions": { "type": "array", "description": "Tool permissions (e.g. Bash, Read)", "default": [] },
                    "append_system_prompt": { "type": "string", "description": "Additional system prompt appended to Claude's instructions" }
                }
            },
            {
                "kind": "slack",
                "node_type": "sink",
                "label": "Slack",
                "config_schema": {
                    "webhook_url_env": { "type": "string", "description": "Env var for webhook URL" },
                    "bot_token_env": { "type": "string", "description": "Env var for bot token" },
                    "channel": { "type": "string", "description": "Channel name (required with bot_token_env)" }
                }
            },
            {
                "kind": "notion",
                "node_type": "sink",
                "label": "Notion",
                "config_schema": {
                    "token_env": { "type": "string", "description": "Env var for Notion token", "required": true },
                    "database_id": { "type": "string", "description": "Notion database ID", "required": true }
                }
            }
        ]
    }))
}

async fn stream_runs(
    State(state): State<AppState>,
    Path(flow_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.events_tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.flow_id != flow_id {
                        continue;
                    }
                    let sse_event_name = event.event_type.as_sse_event();
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().event(sse_event_name).data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(flow_id = %flow_id, skipped = n, "SSE subscriber lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}
