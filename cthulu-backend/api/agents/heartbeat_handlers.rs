use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};

use crate::api::AppState;

/// POST /agents/{id}/wakeup — Manually trigger a heartbeat run
pub async fn wakeup(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.wakeup(&id).await {
        Ok(run) => Ok(Json(serde_json::to_value(&run).unwrap_or_default())),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e})))),
    }
}

/// GET /agents/{id}/heartbeat-runs — List recent heartbeat runs for an agent
pub async fn list_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    let scheduler = state.heartbeat_scheduler.read().await;
    let runs = scheduler.runs_for(&id).await;
    Json(serde_json::to_value(&runs).unwrap_or(json!([])))
}

/// GET /agents/{id}/heartbeat-runs/{run_id} — Get a specific heartbeat run
pub async fn get_run(
    State(state): State<AppState>,
    Path((id, run_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.get_run(&run_id).await {
        Some(run) if run.agent_id == id => Ok(Json(serde_json::to_value(&run).unwrap_or_default())),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// GET /agents/{id}/heartbeat-runs/{run_id}/log — Get the log for a heartbeat run
pub async fn get_run_log(
    State(state): State<AppState>,
    Path((id, run_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.get_run(&run_id).await {
        Some(run) if run.agent_id == id => {
            let content = tokio::fs::read_to_string(&run.log_path).await.unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            Ok(Json(json!({"lines": lines})))
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// GET /agents/claude/status — Check Claude CLI environment health
pub async fn claude_status() -> Json<Value> {
    let result = crate::claude_adapter::probe::test_environment().await;
    Json(serde_json::to_value(&result).unwrap_or_default())
}
