use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;
use cthulu::flows::{Edge, Node};

// ---------------------------------------------------------------------------
// List flows
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_flows(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let flows = state.flow_repo.list_flows().await;

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

    Ok(json!({ "flows": summaries }))
}

// ---------------------------------------------------------------------------
// Get flow
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_flow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let flow = state
        .flow_repo
        .get_flow(&id)
        .await
        .ok_or_else(|| "flow not found".to_string())?;

    serde_json::to_value(&flow).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Create flow
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateFlowRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

#[tauri::command]
pub async fn create_flow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: CreateFlowRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let now = chrono::Utc::now();
    let flow = cthulu::flows::Flow {
        id: uuid::Uuid::new_v4().to_string(),
        name: request.name,
        description: request.description,
        enabled: true,
        nodes: request.nodes,
        edges: request.edges,
        version: 0,
        created_at: now,
        updated_at: now,
    };

    let id = flow.id.clone();
    state
        .flow_repo
        .save_flow(flow)
        .await
        .map_err(|e| format!("failed to save flow: {e}"))?;

    // Start scheduler trigger for the new flow
    if let Err(e) = state.scheduler.start_flow(&id).await {
        tracing::warn!(flow_id = %id, error = %e, "Failed to start trigger for new flow");
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Flow,
        change_type: cthulu::api::changes::ChangeType::Created,
        resource_id: id.clone(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "id": id }))
}

// ---------------------------------------------------------------------------
// Update flow
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdateFlowRequest {
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
    #[serde(default)]
    version: Option<u64>,
}

#[tauri::command]
pub async fn update_flow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    request: UpdateFlowRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let mut flow = state
        .flow_repo
        .get_flow(&id)
        .await
        .ok_or_else(|| "flow not found".to_string())?;

    // Optimistic concurrency: reject stale writes
    if let Some(client_version) = request.version {
        if client_version < flow.version {
            return Err(format!(
                "conflict: server_version={}",
                flow.version
            ));
        }
    }

    if let Some(name) = request.name {
        flow.name = name;
    }
    if let Some(description) = request.description {
        flow.description = description;
    }
    if let Some(enabled) = request.enabled {
        flow.enabled = enabled;
    }
    if let Some(nodes) = request.nodes {
        flow.nodes = nodes;
    }
    if let Some(edges) = request.edges {
        flow.edges = edges;
    }
    flow.version += 1;
    flow.updated_at = chrono::Utc::now();

    state
        .flow_repo
        .save_flow(flow.clone())
        .await
        .map_err(|e| format!("failed to save flow: {e}"))?;

    // Restart scheduler trigger
    if let Err(e) = state.scheduler.restart_flow(&id).await {
        tracing::warn!(flow_id = %id, error = %e, "Failed to restart trigger for updated flow");
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Flow,
        change_type: cthulu::api::changes::ChangeType::Updated,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    serde_json::to_value(&flow).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Delete flow
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_flow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    // Stop scheduler trigger before deleting
    state.scheduler.stop_flow(&id).await;

    let existed = state
        .flow_repo
        .delete_flow(&id)
        .await
        .map_err(|e| format!("failed to delete flow: {e}"))?;

    if !existed {
        return Err("flow not found".to_string());
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Flow,
        change_type: cthulu::api::changes::ChangeType::Deleted,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "deleted": true }))
}

// ---------------------------------------------------------------------------
// Trigger flow
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TriggerFlowRequest {
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    pr: Option<u64>,
}

#[tauri::command]
pub async fn trigger_flow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    request: Option<TriggerFlowRequest>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let flow = state
        .flow_repo
        .get_flow(&id)
        .await
        .ok_or_else(|| "flow not found".to_string())?;

    // Check if this is a PR trigger request
    if let Some(ref trigger_body) = request {
        if let (Some(repo), Some(pr)) = (&trigger_body.repo, trigger_body.pr) {
            let scheduler = state.scheduler.clone();
            let flow_id = id.clone();
            let repo_for_spawn = repo.clone();
            let repo_for_response = repo.clone();

            tokio::spawn(async move {
                if let Err(e) = scheduler.trigger_pr_review(&flow_id, &repo_for_spawn, pr).await {
                    tracing::error!(flow_id = %flow_id, repo = %repo_for_spawn, pr, error = %e, "Manual PR trigger failed");
                }
            });

            return Ok(json!({ "status": "pr_review_started", "flow_id": id, "repo": repo_for_response, "pr": pr }));
        }
    }

    // Default: one-shot flow execution
    let session_bridge = cthulu::flows::session_bridge::SessionBridge {
        sessions: state.interact_sessions.clone(),
        sessions_path: state.sessions_path.clone(),
        data_dir: state.data_dir.clone(),
        session_streams: state.session_streams.clone(),
    };
    let runner = cthulu::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: Some(state.events_tx.clone()),
        sandbox_provider: Some(state.sandbox_provider.clone()),
        agent_repo: Some(state.agent_repo.clone()),
        session_bridge: Some(session_bridge),
    };

    let flow_repo = state.flow_repo.clone();
    let flow_name = flow.name.clone();

    tokio::spawn(async move {
        match runner.execute(&flow, &*flow_repo, None).await {
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

    Ok(json!({ "status": "triggered", "flow_id": id }))
}

// ---------------------------------------------------------------------------
// Get flow runs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_flow_runs(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let runs = state.flow_repo.get_runs(&id, 100).await;
    Ok(json!({ "runs": runs }))
}

// ---------------------------------------------------------------------------
// Get node types
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_node_types(
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    Ok(json!({
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
                    "keywords": { "type": "array", "description": "Filter items by keywords", "default": [] }
                }
            },
            {
                "kind": "web-scrape",
                "node_type": "source",
                "label": "Web Scrape",
                "config_schema": {
                    "url": { "type": "string", "description": "Page URL to scrape", "required": true },
                    "keywords": { "type": "array", "description": "Filter by keywords", "default": [] }
                }
            },
            {
                "kind": "github-merged-prs",
                "node_type": "source",
                "label": "GitHub Merged PRs",
                "config_schema": {
                    "repos": { "type": "array", "description": "Repository slugs", "required": true },
                    "since_days": { "type": "number", "description": "Days to look back", "default": 7 }
                }
            },
            {
                "kind": "web-scraper",
                "node_type": "source",
                "label": "Web Scraper (CSS)",
                "config_schema": {
                    "url": { "type": "string", "description": "Page URL", "required": true },
                    "items_selector": { "type": "string", "description": "CSS selector for items", "required": true },
                    "limit": { "type": "number", "description": "Max items", "default": 10 }
                }
            },
            {
                "kind": "market-data",
                "node_type": "source",
                "label": "Market Data",
                "config_schema": {}
            },
            {
                "kind": "claude-code",
                "node_type": "executor",
                "label": "Claude Code",
                "config_schema": {
                    "agent_id": { "type": "string", "description": "Agent ID", "required": true },
                    "prompt": { "type": "string", "description": "Prompt file or inline", "required": true },
                    "working_dir": { "type": "string", "description": "Working directory", "default": "." }
                }
            },
            {
                "kind": "slack",
                "node_type": "sink",
                "label": "Slack",
                "config_schema": {
                    "webhook_url_env": { "type": "string", "description": "Env var for webhook URL" },
                    "bot_token_env": { "type": "string", "description": "Env var for bot token" },
                    "channel": { "type": "string", "description": "Channel name" }
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

// ---------------------------------------------------------------------------
// List prompt files
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_prompt_files(
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let dir = std::path::Path::new("examples/prompts");
    let mut files: Vec<Value> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let rel_path =
                    format!("examples/prompts/{}", entry.file_name().to_string_lossy());
                let title = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| {
                        content
                            .lines()
                            .find(|l| !l.trim().is_empty())
                            .map(|l| l.trim_start_matches('#').trim().to_string())
                    })
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());

                files.push(json!({
                    "path": rel_path,
                    "filename": entry.file_name().to_string_lossy(),
                    "title": title,
                }));
            }
        }
    }

    Ok(json!({ "files": files }))
}

// ---------------------------------------------------------------------------
// Get flow schedule
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_flow_schedule(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let flow = state
        .flow_repo
        .get_flow(&id)
        .await
        .ok_or_else(|| "flow not found".to_string())?;

    let trigger_node = flow
        .nodes
        .iter()
        .find(|n| n.node_type == cthulu::flows::NodeType::Trigger);

    let Some(trigger) = trigger_node else {
        return Ok(json!({
            "flow_id": id,
            "trigger_kind": null,
            "next_run": null,
            "schedule": null,
        }));
    };

    let trigger_kind = trigger.kind.as_str();

    match trigger_kind {
        "cron" => {
            let schedule = trigger
                .config
                .get("schedule")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if schedule.is_empty() {
                return Ok(json!({
                    "flow_id": id,
                    "trigger_kind": "cron",
                    "schedule": "",
                    "next_run": null,
                    "error": "no schedule configured",
                }));
            }

            match croner::Cron::new(schedule).parse() {
                Ok(cron) => {
                    let now = chrono::Utc::now();
                    let next = cron.find_next_occurrence(&now, false).ok();
                    let mut next_runs = Vec::new();
                    let mut cursor = now;
                    for _ in 0..5 {
                        if let Ok(n) = cron.find_next_occurrence(&cursor, false) {
                            next_runs.push(n.to_rfc3339());
                            cursor = n + chrono::Duration::seconds(1);
                        } else {
                            break;
                        }
                    }

                    Ok(json!({
                        "flow_id": id,
                        "trigger_kind": "cron",
                        "enabled": flow.enabled,
                        "schedule": schedule,
                        "next_run": next.map(|n| n.to_rfc3339()),
                        "next_runs": next_runs,
                    }))
                }
                Err(e) => Ok(json!({
                    "flow_id": id,
                    "trigger_kind": "cron",
                    "schedule": schedule,
                    "next_run": null,
                    "error": format!("invalid cron: {e}"),
                })),
            }
        }
        "github-pr" => {
            let poll_interval = trigger
                .config
                .get("poll_interval")
                .and_then(|v| v.as_u64())
                .unwrap_or(60);
            Ok(json!({
                "flow_id": id,
                "trigger_kind": "github-pr",
                "enabled": flow.enabled,
                "poll_interval_secs": poll_interval,
                "next_run": null,
            }))
        }
        other => Ok(json!({
            "flow_id": id,
            "trigger_kind": other,
            "enabled": flow.enabled,
            "next_run": null,
        })),
    }
}

// ---------------------------------------------------------------------------
// Scheduler status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_scheduler_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let active_ids = state.scheduler.active_flow_ids().await;
    let flows = state.flow_repo.list_flows().await;

    let flow_statuses: Vec<Value> = flows
        .iter()
        .map(|f| {
            let is_active = active_ids.contains(&f.id);
            json!({
                "flow_id": f.id,
                "name": f.name,
                "enabled": f.enabled,
                "scheduler_active": is_active,
            })
        })
        .collect();

    Ok(json!({
        "active_count": active_ids.len(),
        "total_flows": flows.len(),
        "flows": flow_statuses,
    }))
}

// ---------------------------------------------------------------------------
// Validate cron
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ValidateCronRequest {
    expression: String,
}

#[tauri::command]
pub async fn validate_cron(
    ready: tauri::State<'_, crate::ReadySignal>,
    request: ValidateCronRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let expr = request.expression.trim();

    if expr.is_empty() {
        return Ok(json!({
            "valid": false,
            "error": "empty expression",
            "next_runs": [],
        }));
    }

    match croner::Cron::new(expr).parse() {
        Ok(cron) => {
            let now = chrono::Utc::now();
            let mut next_runs = Vec::new();
            let mut cursor = now;
            for _ in 0..5 {
                match cron.find_next_occurrence(&cursor, false) {
                    Ok(n) => {
                        next_runs.push(n.to_rfc3339());
                        cursor = n + chrono::Duration::seconds(1);
                    }
                    Err(_) => break,
                }
            }

            Ok(json!({
                "valid": true,
                "expression": expr,
                "next_runs": next_runs,
            }))
        }
        Err(e) => Ok(json!({
            "valid": false,
            "expression": expr,
            "error": format!("{e}"),
            "next_runs": [],
        })),
    }
}
