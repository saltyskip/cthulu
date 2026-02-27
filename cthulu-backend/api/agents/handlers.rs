use axum::extract::{Path, State};
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
