use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::api::AppState;
use crate::api::changes::{ChangeType, ResourceChangeEvent, ResourceType};
use crate::agents::{Agent, STUDIO_ASSISTANT_ID};

pub(crate) async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    let agents = state.agent_repo.list().await;

    let summaries: Vec<Value> = agents
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.name,
                "description": a.description,
                "permissions": a.permissions,
                "created_at": a.created_at,
                "updated_at": a.updated_at,
            })
        })
        .collect();

    Json(json!({ "agents": summaries }))
}

pub(crate) async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent = state.agent_repo.get(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "agent not found" })),
        )
    })?;

    Ok(Json(serde_json::to_value(&agent).unwrap()))
}

#[derive(Deserialize)]
pub(crate) struct CreateAgentRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    append_system_prompt: Option<String>,
    #[serde(default)]
    working_dir: Option<String>,
}

pub(crate) async fn create_agent(
    State(state): State<AppState>,
    Json(body): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let mut builder = Agent::builder(Uuid::new_v4().to_string())
        .name(body.name)
        .description(body.description)
        .prompt(body.prompt)
        .permissions(body.permissions);
    if let Some(s) = body.append_system_prompt {
        builder = builder.append_system_prompt(s);
    }
    if let Some(w) = body.working_dir {
        builder = builder.working_dir(w);
    }
    let agent = builder.build();

    let id = agent.id.clone();
    if let Err(e) = state.agent_repo.save(agent).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save agent: {e}") })),
        );
    }

    let _ = state.changes_tx.send(ResourceChangeEvent {
        resource_type: ResourceType::Agent,
        change_type: ChangeType::Created,
        resource_id: id.clone(),
        timestamp: Utc::now(),
    });

    (StatusCode::CREATED, Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub(crate) struct UpdateAgentRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    permissions: Option<Vec<String>>,
    #[serde(default)]
    append_system_prompt: Option<Option<String>>,
    #[serde(default)]
    working_dir: Option<Option<String>>,
}

pub(crate) async fn update_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAgentRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut agent = state.agent_repo.get(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "agent not found" })),
        )
    })?;

    if let Some(name) = body.name {
        agent.name = name;
    }
    if let Some(description) = body.description {
        agent.description = description;
    }
    if let Some(prompt) = body.prompt {
        agent.prompt = prompt;
    }
    if let Some(permissions) = body.permissions {
        agent.permissions = permissions;
    }
    if let Some(append_system_prompt) = body.append_system_prompt {
        agent.append_system_prompt = append_system_prompt;
    }
    if let Some(working_dir) = body.working_dir {
        agent.working_dir = working_dir;
    }
    agent.updated_at = Utc::now();

    state.agent_repo.save(agent.clone()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save agent: {e}") })),
        )
    })?;

    let _ = state.changes_tx.send(ResourceChangeEvent {
        resource_type: ResourceType::Agent,
        change_type: ChangeType::Updated,
        resource_id: id,
        timestamp: Utc::now(),
    });

    Ok(Json(serde_json::to_value(&agent).unwrap()))
}

pub(crate) async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if id == STUDIO_ASSISTANT_ID {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "cannot delete the built-in Studio Assistant" })),
        ));
    }

    let existed = state.agent_repo.delete(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to delete agent: {e}") })),
        )
    })?;

    if !existed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "agent not found" })),
        ));
    }

    let _ = state.changes_tx.send(ResourceChangeEvent {
        resource_type: ResourceType::Agent,
        change_type: ChangeType::Deleted,
        resource_id: id,
        timestamp: Utc::now(),
    });

    Ok(Json(json!({ "deleted": true })))
}

// ---------------------------------------------------------------------------
// File explorer (read-only)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FileReadQuery {
    pub path: Option<String>,
}

/// List files in the working directory of a session.
/// Returns a tree of files/directories.
pub(crate) async fn list_session_files(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = format!("agent::{id}");
    let all_sessions = state.interact_sessions.read().await;
    let working_dir = all_sessions
        .get(&key)
        .and_then(|fs| fs.get_session(&session_id))
        .map(|s| s.working_dir.clone());
    drop(all_sessions);

    let working_dir = working_dir.ok_or((
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "session not found" })),
    ))?;

    let dir = std::path::Path::new(&working_dir);
    if !dir.exists() || !dir.is_dir() {
        return Ok(Json(json!({ "tree": [] })));
    }

    let tree = build_file_tree(dir, dir, 20);
    Ok(Json(json!({ "tree": tree, "root": working_dir })))
}

/// Read a single file from the session's working directory (read-only).
pub(crate) async fn read_session_file(
    State(state): State<AppState>,
    Path((id, session_id)): Path<(String, String)>,
    Query(query): Query<FileReadQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let key = format!("agent::{id}");
    let all_sessions = state.interact_sessions.read().await;
    let working_dir = all_sessions
        .get(&key)
        .and_then(|fs| fs.get_session(&session_id))
        .map(|s| s.working_dir.clone());
    drop(all_sessions);

    let working_dir = working_dir.ok_or((
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "session not found" })),
    ))?;

    let rel_path = query.path.unwrap_or_default();
    if rel_path.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "path query parameter is required" })),
        ));
    }

    let base = std::path::Path::new(&working_dir).canonicalize().unwrap_or_else(|_| std::path::PathBuf::from(&working_dir));
    let target = base.join(&rel_path);

    // Security: ensure the resolved path is within the working directory
    let resolved = target.canonicalize().map_err(|_| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "file not found" })))
    })?;
    if !resolved.starts_with(&base) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "path traversal not allowed" })),
        ));
    }

    if !resolved.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not a file" })),
        ));
    }

    // Limit file size to 1MB
    let metadata = std::fs::metadata(&resolved).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "cannot read file metadata" })))
    })?;
    if metadata.len() > 1_048_576 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "file too large (>1MB)" })),
        ));
    }

    let content = std::fs::read_to_string(&resolved).map_err(|_| {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "cannot read file as text (may be binary)" })))
    })?;

    Ok(Json(json!({
        "path": rel_path,
        "content": content,
        "size": metadata.len(),
    })))
}

/// Build a JSON file tree up to `max_depth` levels.
fn build_file_tree(
    dir: &std::path::Path,
    root: &std::path::Path,
    max_depth: u32,
) -> Vec<Value> {
    if max_depth == 0 {
        return vec![];
    }

    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    let mut items: Vec<_> = read_dir.flatten().collect();
    items.sort_by_key(|e| e.file_name());

    for entry in items {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs and common noise
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();

        if path.is_dir() {
            let children = build_file_tree(&path, root, max_depth - 1);
            entries.push(json!({
                "name": name,
                "path": rel,
                "type": "directory",
                "children": children,
            }));
        } else {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            entries.push(json!({
                "name": name,
                "path": rel,
                "type": "file",
                "size": size,
            }));
        }
    }

    entries
}
