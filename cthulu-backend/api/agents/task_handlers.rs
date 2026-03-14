use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agents::heartbeat::WakeupSource;
use crate::agents::tasks::{Task, TaskStatus};
use crate::api::AppState;

// ---------------------------------------------------------------------------
// List tasks
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct ListTasksQuery {
    pub assignee: Option<String>,
}

pub(crate) async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Json<Value> {
    let tasks = if let Some(ref agent_id) = query.assignee {
        state.task_store.list_for_agent(agent_id).await
    } else {
        state.task_store.list().await
    };
    Json(json!({ "tasks": tasks }))
}

// ---------------------------------------------------------------------------
// Create task
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreateTaskRequest {
    title: String,
    assignee_agent_id: String,
}

pub(crate) async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Verify assignee exists
    if state.agent_repo.get(&body.assignee_agent_id).await.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "assignee agent not found" })),
        ));
    }

    let now = Utc::now();
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        title: body.title.clone(),
        status: TaskStatus::Todo,
        assignee_agent_id: body.assignee_agent_id.clone(),
        created_by: "user".into(),
        created_at: now,
        updated_at: now,
    };

    state.task_store.save(task.clone()).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e })))
    })?;

    // Trigger assignment wakeup (best-effort — don't fail task creation if wakeup fails)
    let scheduler = state.heartbeat_scheduler.read().await;
    let task_context = format!("Task: {}", body.title);
    let _ = scheduler
        .wakeup_with_source(&body.assignee_agent_id, WakeupSource::Assignment, Some(&task_context))
        .await;

    Ok((StatusCode::CREATED, Json(serde_json::to_value(&task).unwrap())))
}

// ---------------------------------------------------------------------------
// Update task
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct UpdateTaskRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    assignee_agent_id: Option<String>,
}

pub(crate) async fn update_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<UpdateTaskRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut task = state.task_store.get(&task_id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "task not found" })))
    })?;

    let old_assignee = task.assignee_agent_id.clone();

    if let Some(title) = body.title {
        task.title = title;
    }
    if let Some(status) = body.status {
        task.status = status;
    }
    if let Some(ref assignee) = body.assignee_agent_id {
        if state.agent_repo.get(assignee).await.is_none() {
            return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "assignee agent not found" }))));
        }
        task.assignee_agent_id = assignee.clone();
    }
    task.updated_at = Utc::now();

    state.task_store.save(task.clone()).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e })))
    })?;

    // If assignee changed, wakeup the new assignee
    if let Some(new_assignee) = body.assignee_agent_id {
        if new_assignee != old_assignee {
            let scheduler = state.heartbeat_scheduler.read().await;
            let task_context = format!("Task reassigned to you: {}", task.title);
            let _ = scheduler
                .wakeup_with_source(&new_assignee, WakeupSource::Assignment, Some(&task_context))
                .await;
        }
    }

    Ok(Json(serde_json::to_value(&task).unwrap()))
}

// ---------------------------------------------------------------------------
// Delete task
// ---------------------------------------------------------------------------

pub(crate) async fn delete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let existed = state.task_store.delete(&task_id).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e })))
    })?;

    if !existed {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "task not found" }))));
    }

    Ok(Json(json!({ "deleted": true })))
}
