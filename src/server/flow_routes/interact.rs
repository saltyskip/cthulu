use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use chrono::Utc;
use futures::stream::Stream;
use hyper::StatusCode;
use serde_json::{json, Value};
use std::convert::Infallible;
use uuid::Uuid;

use super::super::AppState;
use super::{resolve_working_dir, make_summary, kill_pid, InteractRequest, StopRequest};

/// GET /flows/{id}/session — resolve session context
pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let vm_mappings_snapshot = state.vm_mappings.read().await.clone();
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
        sandbox_provider: Some(state.sandbox_provider.clone()),
        vm_mappings: vm_mappings_snapshot,
    };

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

/// GET /flows/{id}/interact/sessions — list all sessions for a workflow
pub(crate) async fn list_sessions(
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
pub(crate) async fn new_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let vm_mappings_snapshot = state.vm_mappings.read().await.clone();
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
        sandbox_provider: Some(state.sandbox_provider.clone()),
        vm_mappings: vm_mappings_snapshot,
    };
    let session_info = runner.prepare_session(&flow).await.ok();
    let working_dir = resolve_working_dir(&session_info);

    let new_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let mut all_sessions = state.interact_sessions.write().await;
    let flow_sessions = all_sessions
        .entry(id.clone())
        .or_insert_with(|| super::super::FlowSessions {
            flow_name: flow.name.clone(),
            active_session: String::new(),
            sessions: Vec::new(),
        });

    let warning = if flow_sessions.sessions.len() >= 10 {
        Some("Consider closing old sessions (10+ open)")
    } else {
        None
    };

    flow_sessions.sessions.push(super::super::InteractSession {
        session_id: new_id.clone(),
        summary: String::new(),
        node_id: None,
        working_dir,
        active_pid: None,
        busy: false,
        message_count: 0,
        total_cost: 0.0,
        created_at: now.clone(),
        skills_dir: None,
    });
    flow_sessions.active_session = new_id.clone();
    flow_sessions.flow_name = flow.name.clone();

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_with_vms(&sessions_snapshot);

    let mut resp = json!({ "session_id": new_id, "created_at": now });
    if let Some(w) = warning {
        resp["warning"] = json!(w);
    }
    Ok(Json(resp))
}

/// DELETE /flows/{id}/interact/sessions/{session_id} — remove a specific session
pub(crate) async fn delete_session(
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

        if let Some(session) = flow_sessions.get_session(&session_id) {
            if let Some(pid) = session.active_pid {
                kill_pid(pid);
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
    state.save_sessions_with_vms(&sessions_snapshot);

    Ok(Json(json!({
        "deleted": true,
        "active_session": active_after,
    })))
}

/// POST /flows/{id}/interact — SSE stream for flow-level chat
pub(crate) async fn interact_flow(
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

    let vm_mappings_snapshot = state.vm_mappings.read().await.clone();
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: None,
        sandbox_provider: Some(state.sandbox_provider.clone()),
        vm_mappings: vm_mappings_snapshot,
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

    let (target_session_id, is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;

        let flow_sessions = all_sessions
            .entry(id.clone())
            .or_insert_with(|| {
                let sid = Uuid::new_v4().to_string();
                super::super::FlowSessions {
                    flow_name: flow.name.clone(),
                    active_session: sid.clone(),
                    sessions: vec![super::super::InteractSession {
                        session_id: sid,
                        summary: make_summary(&prompt),
                        node_id: None,
                        working_dir: default_working_dir.clone(),
                        active_pid: None,
                        busy: false,
                        message_count: 0,
                        total_cost: 0.0,
                        created_at: Utc::now().to_rfc3339(),
                        skills_dir: None,
                    }],
                }
            });

        flow_sessions.flow_name = flow.name.clone();

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
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "session is busy processing a previous message" })),
            ));
        }

        let is_new = session.message_count == 0;

        if is_new && session.summary.is_empty() {
            session.summary = make_summary(&prompt);
        }

        session.busy = true;
        let sid = session.session_id.clone();
        let wdir = session.working_dir.clone();

        flow_sessions.active_session = sid.clone();

        let sessions_snapshot = all_sessions.clone();
        drop(all_sessions);
        state.save_sessions_with_vms(&sessions_snapshot);

        (sid, is_new, wdir)
    };

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
    let vm_mappings_ref = state.vm_mappings.clone();

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

        args.push("-".to_string());

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

        if let Some(pid) = child.id() {
            let mut all_sessions = sessions_ref.write().await;
            if let Some(fs) = all_sessions.get_mut(&flow_id_for_stream) {
                if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                    s.active_pid = Some(pid);
                }
            }
        }

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

        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut session_cost: f64 = 0.0;

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }

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
                yield Ok(Event::default().event("text").data(
                    serde_json::to_string(&json!({"text": line})).unwrap()
                ));
            }
        }

        while let Ok(err_line) = stderr_rx.try_recv() {
            yield Ok(Event::default().event("stderr").data(err_line));
        }

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
            let sessions_snapshot = all_sessions.clone();
            drop(all_sessions);
            let vms = vm_mappings_ref.try_read()
                .map(|g| g.clone())
                .unwrap_or_default();
            crate::server::save_sessions(&sessions_path, &sessions_snapshot, &vms);
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

/// POST /flows/{id}/interact/reset — clear ALL sessions, create one fresh session
pub(crate) async fn reset_interact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&id) {
        for session in &flow_sessions.sessions {
            if let Some(pid) = session.active_pid {
                kill_pid(pid);
            }
        }
        flow_sessions.sessions.clear();
        flow_sessions.active_session = String::new();
    }
    all_sessions.remove(&id);
    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_with_vms(&sessions_snapshot);
    Ok(Json(json!({ "status": "reset" })))
}

/// POST /flows/{id}/interact/stop — stop the active (or specified) session's process
pub(crate) async fn stop_interact(
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
