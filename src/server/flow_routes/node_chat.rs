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
use super::super::FlowSessions;
use super::super::InteractSession;
use super::super::LiveClaudeProcess;
use super::super::node_sessions_key;
use super::{make_summary, kill_pid, attachments_path, InteractRequest, StopRequest};
use crate::flows::{Edge, Flow, Node, NodeType};

// ---------------------------------------------------------------------------
// Node-level chat endpoints
// ---------------------------------------------------------------------------

/// GET /flows/{id}/nodes/{node_id}/session — resolve node config for chat
pub(crate) async fn get_node_session(
    State(state): State<AppState>,
    Path((id, node_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" })))
    })?;

    match crate::flows::runner::FlowRunner::prepare_node_session(&flow, &node_id) {
        Ok(info) => Ok(Json(serde_json::to_value(&info).unwrap())),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("{e:#}") })),
        )),
    }
}

/// GET /flows/{id}/nodes/{node_id}/interact/sessions — list sessions for this node
pub(crate) async fn list_node_sessions(
    State(state): State<AppState>,
    Path((id, node_id)): Path<(String, String)>,
) -> Json<Value> {
    let key = node_sessions_key(&id, &node_id);
    let sessions = state.interact_sessions.read().await;
    if let Some(flow_sessions) = sessions.get(&key) {
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

/// POST /flows/{id}/nodes/{node_id}/interact/new — create a new node session tab
pub(crate) async fn new_node_session(
    State(state): State<AppState>,
    Path((id, node_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" })))
    })?;

    let session_info = crate::flows::runner::FlowRunner::prepare_node_session(&flow, &node_id).ok();
    let working_dir = session_info
        .as_ref()
        .map(|s| s.working_dir.clone())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .to_string()
        });

    let key = node_sessions_key(&id, &node_id);
    let new_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let mut all_sessions = state.interact_sessions.write().await;
    let flow_sessions = all_sessions
        .entry(key)
        .or_insert_with(|| FlowSessions {
            flow_name: format!("{} [{}]", flow.name, node_id),
            active_session: String::new(),
            sessions: Vec::new(),
        });

    let warning = if flow_sessions.sessions.len() >= 10 {
        Some("Consider closing old sessions (10+ open)")
    } else {
        None
    };

    flow_sessions.sessions.push(InteractSession {
        session_id: new_id.clone(),
        summary: String::new(),
        node_id: Some(node_id.clone()),
        working_dir,
        active_pid: None,
        busy: false,
        message_count: 0,
        total_cost: 0.0,
        created_at: now.clone(),
        skills_dir: None,
    });
    flow_sessions.active_session = new_id.clone();

    let sessions_snapshot = all_sessions.clone();
    drop(all_sessions);
    state.save_sessions_with_vms(&sessions_snapshot);

    let mut resp = json!({ "session_id": new_id, "created_at": now });
    if let Some(w) = warning {
        resp["warning"] = json!(w);
    }
    Ok(Json(resp))
}

/// DELETE /flows/{id}/nodes/{node_id}/interact/sessions/{session_id}
pub(crate) async fn delete_node_session(
    State(state): State<AppState>,
    Path((id, node_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = node_sessions_key(&id, &node_id);
    let mut all_sessions = state.interact_sessions.write().await;

    let active_after = {
        let flow_sessions = all_sessions.get_mut(&key).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no sessions for this node" })))
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

/// POST /flows/{id}/nodes/{node_id}/interact/stop
pub(crate) async fn stop_node_interact(
    State(state): State<AppState>,
    Path((id, node_id)): Path<(String, String)>,
    body: Option<Json<StopRequest>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = node_sessions_key(&id, &node_id);

    // Remove the persistent process from the pool and kill it
    {
        let mut pool = state.live_processes.lock().await;
        if let Some(mut proc) = pool.remove(&key) {
            let _ = proc.child.kill().await;
        }
    }

    let mut all_sessions = state.interact_sessions.write().await;
    if let Some(flow_sessions) = all_sessions.get_mut(&key) {
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
        "keyword" => {
            let kw = node.config.get("keywords").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "?".into());
            format!("keywords: {kw}")
        }
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

/// Walk edges to find all upstream node IDs (recursive breadth-first).
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

/// Walk edges to find all downstream node IDs (recursive breadth-first).
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

    // Find upstream and downstream
    let upstream_ids = find_upstream(node_id, &flow.edges, 0);
    let downstream_ids = find_downstream(node_id, &flow.edges, 0);

    // Build pipeline visualization
    let mut pipeline_lines = Vec::new();

    // Triggers
    for n in &flow.nodes {
        if n.node_type == NodeType::Trigger {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("  {marker}Trigger: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    // Sources
    for n in &flow.nodes {
        if n.node_type == NodeType::Source {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("    -> {marker}Source: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    // Filters
    for n in &flow.nodes {
        if n.node_type == NodeType::Filter {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("    -> {marker}Filter: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    // Executors
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

    // Sinks
    for n in &flow.nodes {
        if n.node_type == NodeType::Sink {
            let marker = if n.id == node_id { "**" } else { "" };
            pipeline_lines.push(format!("    -> {marker}Sink: {} ({}) -- {}{marker}",
                n.label, n.kind, node_config_summary(n)));
        }
    }

    // Build upstream table
    let mut upstream_table = String::new();
    for uid in &upstream_ids {
        if let Some(n) = node_map.get(uid.as_str()) {
            upstream_table.push_str(&format!(
                "| {} | {} | {} |\n",
                n.label, n.kind, node_config_summary(n)
            ));
        }
    }

    // Build downstream table
    let mut downstream_table = String::new();
    for did in &downstream_ids {
        if let Some(n) = node_map.get(did.as_str()) {
            downstream_table.push_str(&format!(
                "| {} | {} | {} |\n",
                n.label, n.kind, node_config_summary(n)
            ));
        }
    }

    // Build executor list
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

    // Get prompt path for this node
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

/// POST /flows/{id}/nodes/{node_id}/interact — SSE stream for node-level chat
pub(crate) async fn interact_node(
    State(state): State<AppState>,
    Path((id, node_id)): Path<(String, String)>,
    Json(body): Json<InteractRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let flow = state.store.get_flow(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" })))
    })?;

    let prompt = body.prompt;
    if prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "prompt is required" })),
        ));
    }

    // Resolve node config
    let session_info = crate::flows::runner::FlowRunner::prepare_node_session(&flow, &node_id).ok();

    let permissions: Vec<String> = session_info
        .as_ref()
        .map(|s| s.permissions.clone())
        .unwrap_or_default();
    let append_system_prompt = session_info
        .as_ref()
        .and_then(|s| s.append_system_prompt.clone());

    let default_working_dir = session_info
        .as_ref()
        .map(|s| s.working_dir.clone())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .to_string()
        });

    let key = node_sessions_key(&id, &node_id);

    // Look up or create the session
    let (target_session_id, is_new, working_dir) = {
        let mut all_sessions = state.interact_sessions.write().await;

        let flow_sessions = all_sessions
            .entry(key.clone())
            .or_insert_with(|| {
                let sid = Uuid::new_v4().to_string();
                FlowSessions {
                    flow_name: format!("{} [{}]", flow.name, node_id),
                    active_session: sid.clone(),
                    sessions: vec![InteractSession {
                        session_id: sid,
                        summary: make_summary(&prompt),
                        node_id: Some(node_id.clone()),
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

    // On first message: generate .skills/ context files and build system prompt
    let data_dir = state.data_dir.clone();
    let flow_id_for_attach = id.clone();
    let node_id_for_attach = node_id.clone();

    let system_prompt = if is_new {
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

        // 2. Generate Skill.md (workflow context)
        let skill_md = build_workflow_context_md(&flow, &node_id);
        let _ = std::fs::write(skills_dir.join("Skill.md"), &skill_md);

        // 3. Write workflow.json (full flow definition)
        if let Ok(flow_json) = serde_json::to_string_pretty(&flow) {
            let _ = std::fs::write(skills_dir.join("workflow.json"), &flow_json);
        }

        // 4. Sync user-uploaded attachments to .skills/
        let att_dir = attachments_path(&data_dir, &flow_id_for_attach, &node_id_for_attach);
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

        // 5. Store skills_dir path in session
        {
            let mut all_sessions = state.interact_sessions.write().await;
            if let Some(fs) = all_sessions.get_mut(&key) {
                if let Some(s) = fs.get_session_mut(&target_session_id) {
                    s.skills_dir = Some(skills_dir.to_string_lossy().to_string());
                }
            }
            let sessions_snapshot = all_sessions.clone();
            drop(all_sessions);
            state.save_sessions_with_vms(&sessions_snapshot);
        }

        // 6. Build concise system prompt referencing .skills/ files
        let node_label = flow.nodes.iter()
            .find(|n| n.id == node_id)
            .map(|n| n.label.as_str())
            .unwrap_or("Executor");

        let mut sys_prompt = format!(
            "You are \"{node_label}\", an executor agent in the \"{flow_name}\" workflow.\n\n\
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
            node_label = node_label,
            flow_name = flow.name,
        );

        if let Some(ref extra) = append_system_prompt {
            if !extra.is_empty() {
                sys_prompt.push_str(&format!("\n\n{extra}"));
            }
        }

        Some(sys_prompt)
    } else {
        None
    };

    let key_for_stream = key.clone();
    let session_id_for_stream = target_session_id.clone();
    let sessions_ref = state.interact_sessions.clone();
    let sessions_path = state.sessions_path.clone();
    let vm_mappings_ref = state.vm_mappings.clone();
    let live_processes = state.live_processes.clone();

    let stream = async_stream::stream! {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        // Check if we have a live persistent process for this session
        let needs_spawn = {
            let pool = live_processes.lock().await;
            !pool.contains_key(&key_for_stream)
        };

        if needs_spawn {
            // Spawn a new persistent Claude process with stream-json input
            let mut args = vec![
                "--print".to_string(),
                "--verbose".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--input-format".to_string(),
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

            tracing::info!(
                key = %key_for_stream,
                session_id = %session_id_for_stream,
                is_new,
                "spawning persistent claude for node interact"
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
                    tracing::error!(error = %e, "failed to spawn claude for node interact");
                    let mut all_sessions = sessions_ref.write().await;
                    if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
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
                if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                    if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                        s.active_pid = Some(pid);
                    }
                }
            }

            let child_stdin = child.stdin.take().expect("stdin piped");

            // Spawn stdout reader task that pipes lines into a channel
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

            // Spawn stderr reader task
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

            let mut pool = live_processes.lock().await;
            pool.insert(key_for_stream.clone(), live_proc);

            yield Ok(Event::default().event("system").data(
                serde_json::to_string(&json!({"message": "Session started"})).unwrap()
            ));
        } else {
            yield Ok(Event::default().event("system").data(
                serde_json::to_string(&json!({"message": "Ready"})).unwrap()
            ));
        }

        // Write the prompt to the persistent process's stdin as stream-json
        {
            let mut pool = live_processes.lock().await;
            if let Some(proc) = pool.get_mut(&key_for_stream) {
                // stream-json input format: one JSON object per line
                // Format: {"type":"user","message":{"role":"user","content":"..."}}
                let input_msg = serde_json::to_string(&json!({
                    "type": "user",
                    "message": {
                        "role": "user",
                        "content": prompt,
                    }
                })).unwrap();
                let write_result = proc.stdin.write_all(format!("{input_msg}\n").as_bytes()).await;
                if let Err(e) = write_result {
                    tracing::error!(error = %e, "failed to write to persistent claude stdin");
                    // Process likely died — remove it so next call spawns fresh
                    pool.remove(&key_for_stream);
                    let mut all_sessions = sessions_ref.write().await;
                    if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                        if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                            s.busy = false;
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

        // Read output lines from the process until we get a "result" event
        let mut session_cost: f64 = 0.0;
        let mut got_result = false;

        loop {
            let (line, stderr_batch) = {
                let mut pool = live_processes.lock().await;
                if let Some(proc) = pool.get_mut(&key_for_stream) {
                    // Drain stderr into a vec (can't yield inside lock)
                    let mut errs = Vec::new();
                    while let Ok(err_line) = proc.stderr_lines.try_recv() {
                        errs.push(err_line);
                    }
                    // Try to get a stdout line
                    let stdout_line = proc.stdout_lines.try_recv().ok();
                    (stdout_line, errs)
                } else {
                    break;
                }
            };

            // Yield collected stderr lines outside the lock
            for err_line in stderr_batch {
                tracing::debug!(stderr = %err_line, "claude stderr");
                yield Ok(Event::default().event("stderr").data(err_line));
            }

            if let Some(line) = line {
                if line.is_empty() {
                    continue;
                }

                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&line) {
                    let event_type = json_val.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                    match event_type {
                        "system" => {
                            // Skip system events on resume — process is already alive
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
                            got_result = true;
                        }
                        _ => {}
                    }
                } else {
                    yield Ok(Event::default().event("text").data(
                        serde_json::to_string(&json!({"text": line})).unwrap()
                    ));
                }

                if got_result {
                    break;
                }
            } else {
                // No line available yet — yield briefly to avoid busy-waiting
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

                // Check if process is still alive
                let mut pool = live_processes.lock().await;
                if let Some(proc) = pool.get_mut(&key_for_stream) {
                    if let Ok(Some(_status)) = proc.child.try_wait() {
                        // Process exited — clean up
                        pool.remove(&key_for_stream);
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
            if let Some(proc) = pool.get_mut(&key_for_stream) {
                proc.busy = false;
            }
        }

        {
            let mut all_sessions = sessions_ref.write().await;
            if let Some(fs) = all_sessions.get_mut(&key_for_stream) {
                if let Some(s) = fs.get_session_mut(&session_id_for_stream) {
                    s.busy = false;
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

        yield Ok(Event::default().event("done").data(
            serde_json::to_string(&json!({"exit_code": 0})).unwrap()
        ));
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}
