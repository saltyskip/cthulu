use serde_json::{json, Value};

use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// Wakeup (manual heartbeat trigger)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn wakeup_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.wakeup(&agent_id).await {
        Ok(run) => serde_json::to_value(&run).map_err(|e| e.to_string()),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// List heartbeat runs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_heartbeat_runs(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let scheduler = state.heartbeat_scheduler.read().await;
    let runs = scheduler.runs_for(&agent_id).await;
    serde_json::to_value(&runs).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Get heartbeat run
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_heartbeat_run(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    run_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.get_run(&run_id).await {
        Some(run) if run.agent_id == agent_id => {
            serde_json::to_value(&run).map_err(|e| e.to_string())
        }
        _ => Err("heartbeat run not found".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Get heartbeat run log
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_heartbeat_run_log(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    agent_id: String,
    run_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let scheduler = state.heartbeat_scheduler.read().await;
    match scheduler.get_run(&run_id).await {
        Some(run) if run.agent_id == agent_id => {
            let content = tokio::fs::read_to_string(&run.log_path)
                .await
                .unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            Ok(json!({ "lines": lines }))
        }
        _ => Err("heartbeat run not found".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Claude CLI status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn claude_status(
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let result = cthulu::claude_adapter::probe::test_environment().await;
    serde_json::to_value(&result).map_err(|e| e.to_string())
}
