use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use chrono::Utc;
use futures::stream::Stream;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use uuid::Uuid;

use crate::api::AppState;
use crate::flows::{Edge, Flow, Node};

pub(crate) async fn list_flows(State(state): State<AppState>) -> Json<Value> {
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

    Json(json!({ "flows": summaries }))
}

pub(crate) async fn get_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let flow = state.flow_repo.get_flow(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    Ok(Json(serde_json::to_value(&flow).unwrap()))
}

#[derive(Deserialize)]
pub(crate) struct CreateFlowRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

pub(crate) async fn create_flow(
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
    if let Err(e) = state.flow_repo.save_flow(flow).await {
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
pub(crate) struct UpdateFlowRequest {
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

pub(crate) async fn update_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFlowRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut flow = state.flow_repo.get_flow(&id).await.ok_or_else(|| {
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

    state.flow_repo.save_flow(flow.clone()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        )
    })?;

    // Restart scheduler trigger (handles enable/disable/config changes)
    if let Err(e) = state.scheduler.restart_flow(&id).await {
        tracing::warn!(flow_id = %id, error = %e, "Failed to restart trigger for updated flow");
    }

    // VM lifecycle: provision on enable, destroy on disable
    if body.enabled.is_some() {
        if let Some(vm_manager) = &state.vm_manager {
            if flow.enabled {
                // Flow was enabled — provision VMs for all executor nodes
                let oauth_arc = state.oauth_token.clone();
                let vm_mgr = vm_manager.clone();
                let flow_clone = flow.clone();
                let vm_mappings = state.vm_mappings.clone();
                let sessions_path = state.sessions_path.clone();
                let interact_sessions = state.interact_sessions.clone();

                tokio::spawn(async move {
                    let oauth = oauth_arc.read().await.clone();
                    let credentials_json = crate::api::auth::handlers::read_full_credentials();
                    match vm_mgr.provision_flow_vms(&flow_clone, oauth.as_deref(), credentials_json.as_deref()).await {
                        Ok(results) => {
                            let mut map = vm_mappings.write().await;
                            for (node_id, vm_name, vm) in results {
                                let key = format!("{}::{}", flow_clone.id, node_id);
                                map.insert(key, crate::api::VmMapping {
                                    vm_id: vm.vm_id,
                                    vm_name,
                                    web_terminal_url: vm.web_terminal.clone(),
                                });
                            }
                            // Persist
                            let sessions = interact_sessions.read().await.clone();
                            crate::api::save_sessions(&sessions_path, &sessions, &map);
                            tracing::info!(flow = %flow_clone.name, "VMs provisioned for enabled flow");
                        }
                        Err(e) => {
                            tracing::error!(flow = %flow_clone.name, error = %e, "Failed to provision VMs");
                        }
                    }
                });
            } else {
                // Flow was disabled — destroy VMs
                let vm_mgr = vm_manager.clone();
                let flow_clone = flow.clone();
                let vm_mappings = state.vm_mappings.clone();
                let sessions_path = state.sessions_path.clone();
                let interact_sessions = state.interact_sessions.clone();

                tokio::spawn(async move {
                    if let Err(e) = vm_mgr.destroy_flow_vms(&flow_clone).await {
                        tracing::error!(flow = %flow_clone.name, error = %e, "Failed to destroy VMs");
                    }
                    // Remove VM mappings for this flow
                    let mut map = vm_mappings.write().await;
                    let prefix = format!("{}::", flow_clone.id);
                    map.retain(|k, _| !k.starts_with(&prefix));
                    // Persist
                    let sessions = interact_sessions.read().await.clone();
                    crate::api::save_sessions(&sessions_path, &sessions, &map);
                    tracing::info!(flow = %flow_clone.name, "VMs destroyed for disabled flow");
                });
            }
        }
    }

    Ok(Json(serde_json::to_value(&flow).unwrap()))
}

pub(crate) async fn delete_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Stop scheduler trigger before deleting
    state.scheduler.stop_flow(&id).await;

    let existed = state.flow_repo.delete_flow(&id).await.map_err(|e| {
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
pub(crate) struct TriggerFlowRequest {
    repo: Option<String>,
    pr: Option<u64>,
}

pub(crate) async fn trigger_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: String,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let flow = state.flow_repo.get_flow(&id).await.ok_or_else(|| {
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
    let vm_mappings_snapshot = state.vm_mappings.read().await.clone();
    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: Some(state.events_tx.clone()),
        sandbox_provider: Some(state.sandbox_provider.clone()),
        vm_mappings: vm_mappings_snapshot,
        agent_repo: Some(state.agent_repo.clone()),
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

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "triggered", "flow_id": id })),
    ))
}

pub(crate) async fn get_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let runs = state.flow_repo.get_runs(&id, 100).await;
    Json(json!({ "runs": runs }))
}

pub(crate) async fn stream_runs(
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

pub(crate) async fn get_node_types() -> Json<Value> {
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
                "kind": "vm-sandbox",
                "node_type": "executor",
                "label": "VM Sandbox",
                "config_schema": {
                    "tier": { "type": "string", "description": "VM tier: nano (1 vCPU, 512MB) or micro (2 vCPU, 1024MB)", "default": "nano" },
                    "prompt": { "type": "string", "description": "Prompt template (inline or file path)" },
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

/// GET /api/prompt-files — list prompt files from examples/prompts/ directory.
pub(crate) async fn list_prompt_files() -> Json<Value> {
    Json(serde_json::json!({ "files": list_prompt_files_impl() }))
}

fn list_prompt_files_impl() -> Vec<Value> {
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
                        content.lines().find(|l| !l.trim().is_empty()).map(|l| {
                            l.trim_start_matches('#').trim().to_string()
                        })
                    })
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());

                files.push(serde_json::json!({
                    "path": rel_path,
                    "filename": entry.file_name().to_string_lossy(),
                    "title": title,
                }));
            }
        }
    }

    files
}
