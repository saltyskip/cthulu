use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::agents::heartbeat::WakeupSource;
use cthulu::agents::tasks::{Task, TaskStatus};
use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// List tasks
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_tasks(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    assignee: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let tasks = if let Some(ref agent_id) = assignee {
        state.task_store.list_for_agent(agent_id).await
    } else {
        state.task_store.list().await
    };
    Ok(json!({ "tasks": tasks }))
}

// ---------------------------------------------------------------------------
// Create task
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    title: String,
    assignee_agent_id: String,
}

#[tauri::command]
pub async fn create_task(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: CreateTaskRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    if state.agent_repo.get(&request.assignee_agent_id).await.is_none() {
        return Err("assignee agent not found".to_string());
    }

    let now = chrono::Utc::now();
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        title: request.title.clone(),
        status: TaskStatus::Todo,
        assignee_agent_id: request.assignee_agent_id.clone(),
        created_by: "user".into(),
        created_at: now,
        updated_at: now,
    };

    state.task_store.save(task.clone()).await?;

    // Trigger assignment wakeup (best-effort)
    let scheduler = state.heartbeat_scheduler.read().await;
    let task_context = format!("Task: {}", request.title);
    let _ = scheduler
        .wakeup_with_source(&request.assignee_agent_id, WakeupSource::Assignment, Some(&task_context))
        .await;

    serde_json::to_value(&task).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Update task
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    assignee_agent_id: Option<String>,
}

#[tauri::command]
pub async fn update_task(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    request: UpdateTaskRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    let mut task = state.task_store.get(&id).await
        .ok_or_else(|| "task not found".to_string())?;

    let old_assignee = task.assignee_agent_id.clone();

    if let Some(title) = request.title {
        task.title = title;
    }
    if let Some(status) = request.status {
        task.status = status;
    }
    if let Some(ref assignee) = request.assignee_agent_id {
        if state.agent_repo.get(assignee).await.is_none() {
            return Err("assignee agent not found".to_string());
        }
        task.assignee_agent_id = assignee.clone();
    }
    task.updated_at = chrono::Utc::now();

    state.task_store.save(task.clone()).await?;

    // If assignee changed, wakeup new assignee
    if let Some(new_assignee) = request.assignee_agent_id {
        if new_assignee != old_assignee {
            let scheduler = state.heartbeat_scheduler.read().await;
            let task_context = format!("Task reassigned to you: {}", task.title);
            let _ = scheduler
                .wakeup_with_source(&new_assignee, WakeupSource::Assignment, Some(&task_context))
                .await;
        }
    }

    serde_json::to_value(&task).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Delete task
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_task(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let existed = state.task_store.delete(&id).await?;
    if !existed {
        return Err("task not found".to_string());
    }
    Ok(json!({ "deleted": true }))
}
