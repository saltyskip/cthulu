//! Teams: flat groups of users. No roles — all members are equal.
//! Stored in `{data_dir}/teams.json`.

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::api::local_auth::AuthUser;
use crate::api::AppState;

// ── Team Model ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub members: Vec<String>,
    pub created_at: String,
}

/// In-memory team store backed by `{data_dir}/teams.json`.
pub struct TeamStore {
    pub teams: HashMap<String, Team>,
}

impl TeamStore {
    pub fn load(data_dir: &PathBuf) -> Self {
        let path = data_dir.join("teams.json");
        let teams = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Self { teams }
    }

    pub fn save(&self, data_dir: &PathBuf) -> std::io::Result<()> {
        let path = data_dir.join("teams.json");
        let json = serde_json::to_string_pretty(&self.teams)?;
        std::fs::write(path, json)
    }

    pub fn teams_for_user(&self, user_id: &str) -> Vec<&Team> {
        self.teams.values()
            .filter(|t| t.members.contains(&user_id.to_string()))
            .collect()
    }
}

// ── Routes ───────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams", get(list_teams).post(create_team))
        .route("/teams/{id}", get(get_team))
        .route("/teams/{id}/members", post(add_member))
        .route("/teams/{id}/members/{user_id}", delete(remove_member))
}

// ── Handlers ─────────────────────────────────────────────────

async fn list_teams(
    auth: AuthUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    let store = state.team_store.read().await;
    let teams: Vec<&Team> = store.teams_for_user(&auth.user_id);
    (StatusCode::OK, Json(json!({ "teams": teams })))
}

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
}

async fn create_team(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTeamRequest>,
) -> (StatusCode, Json<Value>) {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Team name required" })));
    }

    let team = Team {
        id: uuid::Uuid::new_v4().to_string().replace('-', "_"),
        name,
        created_by: auth.user_id.clone(),
        members: vec![auth.user_id.clone()],
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let mut store = state.team_store.write().await;
    let response = json!({ "team": &team });
    store.teams.insert(team.id.clone(), team);
    let _ = store.save(&state.data_dir);

    (StatusCode::CREATED, Json(response))
}

async fn get_team(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let store = state.team_store.read().await;
    match store.teams.get(&id) {
        Some(team) if team.members.contains(&auth.user_id) => {
            let user_store = state.user_store.read().await;
            let members: Vec<Value> = team.members.iter().map(|uid| {
                let user = user_store.users.values().find(|u| u.id == *uid);
                json!({
                    "id": uid,
                    "email": user.map(|u| u.email.as_str()).unwrap_or("unknown"),
                    "name": user.and_then(|u| u.name.as_deref()),
                })
            }).collect();

            (StatusCode::OK, Json(json!({
                "team": {
                    "id": team.id,
                    "name": team.name,
                    "created_by": team.created_by,
                    "members": members,
                    "created_at": team.created_at,
                }
            })))
        }
        Some(_) => (StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member of this team" }))),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" }))),
    }
}

#[derive(Deserialize)]
struct AddMemberRequest {
    email: String,
}

async fn add_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AddMemberRequest>,
) -> (StatusCode, Json<Value>) {
    let email = body.email.trim().to_lowercase();

    let user_store = state.user_store.read().await;
    let target_user = user_store.find_by_email(&email);
    let target_id = match target_user {
        Some(u) => u.id.clone(),
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))),
    };
    drop(user_store);

    let mut store = state.team_store.write().await;
    match store.teams.get_mut(&id) {
        Some(team) if team.members.contains(&auth.user_id) => {
            if team.members.contains(&target_id) {
                return (StatusCode::CONFLICT, Json(json!({ "error": "Already a member" })));
            }
            team.members.push(target_id);
            let _ = store.save(&state.data_dir);
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Some(_) => (StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member of this team" }))),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" }))),
    }
}

async fn remove_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let mut store = state.team_store.write().await;
    match store.teams.get_mut(&id) {
        Some(team) if team.members.contains(&auth.user_id) => {
            team.members.retain(|m| m != &user_id);
            if team.members.is_empty() {
                store.teams.remove(&id);
            }
            let _ = store.save(&state.data_dir);
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Some(_) => (StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member of this team" }))),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" }))),
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_store_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();

        let mut store = TeamStore { teams: HashMap::new() };
        store.teams.insert("t1".into(), Team {
            id: "t1".into(),
            name: "Alpha".into(),
            created_by: "u1".into(),
            members: vec!["u1".into(), "u2".into()],
            created_at: "2026-01-01".into(),
        });
        store.save(&dir).unwrap();

        let loaded = TeamStore::load(&dir);
        assert_eq!(loaded.teams.len(), 1);
        assert_eq!(loaded.teams["t1"].name, "Alpha");
        assert_eq!(loaded.teams["t1"].members.len(), 2);
    }

    #[test]
    fn teams_for_user_filters_correctly() {
        let mut store = TeamStore { teams: HashMap::new() };
        store.teams.insert("t1".into(), Team {
            id: "t1".into(), name: "A".into(), created_by: "u1".into(),
            members: vec!["u1".into(), "u2".into()], created_at: "".into(),
        });
        store.teams.insert("t2".into(), Team {
            id: "t2".into(), name: "B".into(), created_by: "u3".into(),
            members: vec!["u3".into()], created_at: "".into(),
        });

        assert_eq!(store.teams_for_user("u1").len(), 1);
        assert_eq!(store.teams_for_user("u2").len(), 1);
        assert_eq!(store.teams_for_user("u3").len(), 1);
        assert_eq!(store.teams_for_user("u99").len(), 0);
    }
}
