use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
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
        .route("/flows/{id}/runs", get(get_runs))
        .route("/node-types", get(get_node_types))
}

async fn list_flows(State(state): State<AppState>) -> Json<Value> {
    let flows = state.flow_store.list().await;

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
    let flow = state.flow_store.get(&id).await.ok_or_else(|| {
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
    if let Err(e) = state.flow_store.save(flow).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        );
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
    let mut flow = state.flow_store.get(&id).await.ok_or_else(|| {
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

    state.flow_store.save(flow.clone()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        )
    })?;

    Ok(Json(serde_json::to_value(&flow).unwrap()))
}

async fn delete_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let existed = state.flow_store.delete(&id).await.map_err(|e| {
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

async fn trigger_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let flow = state.flow_store.get(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "flow not found" })),
        )
    })?;

    let runner = crate::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
    };

    let history = state.run_history.clone();
    let flow_name = flow.name.clone();

    tokio::spawn(async move {
        match runner.execute(&flow, &history).await {
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

async fn get_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let runs = state.run_history.get_runs(&id).await;
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
                    "review_on_push": { "type": "boolean", "default": false }
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
                    "limit": { "type": "number", "description": "Max items to fetch", "default": 10 }
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
                    "prompt": { "type": "string", "description": "Prompt file path or inline prompt", "required": true },
                    "permissions": { "type": "array", "description": "Tool permissions (e.g. Bash, Read)", "default": [] }
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
