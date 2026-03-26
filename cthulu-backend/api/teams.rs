//! Teams — flat groups of users with CRUD and member management.
//!
//! Teams are persisted in `{data_dir}/teams.json`. All endpoints require auth.
//! No roles for now — all members are equal.

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::api::local_auth::AuthUser;
use crate::api::AppState;

// ── Types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub members: Vec<String>, // user IDs
    pub created_at: String,
}

// ── Store ────────────────────────────────────────────────

pub struct TeamStore {
    pub teams: HashMap<String, Team>,
    data_dir: std::path::PathBuf,
}

impl TeamStore {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("teams.json");
        let teams = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Self {
            teams,
            data_dir: data_dir.to_path_buf(),
        }
    }

    fn save(&self) {
        let path = self.data_dir.join("teams.json");
        let tmp = path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string_pretty(&self.teams) {
            let _ = std::fs::write(&tmp, json);
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// List teams that contain the given user_id.
    pub fn list_for_user(&self, user_id: &str) -> Vec<&Team> {
        self.teams
            .values()
            .filter(|t| t.members.contains(&user_id.to_string()))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Team> {
        self.teams.get(id)
    }

    pub fn insert(&mut self, team: Team) {
        self.teams.insert(team.id.clone(), team);
        self.save();
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let removed = self.teams.remove(id).is_some();
        if removed {
            self.save();
        }
        removed
    }
}

// ── Router ───────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams", get(list_teams))
        .route("/teams", post(create_team))
        .route("/teams/{id}", get(get_team))
        .route("/teams/{id}/members", post(add_member))
        .route("/teams/{id}/members/{user_id}", delete(remove_member))
}

// ── Handlers ─────────────────────────────────────────────

/// GET /api/teams — list teams the user belongs to
async fn list_teams(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Json<Value> {
    let store = state.team_store.read().await;
    let teams: Vec<Value> = store
        .list_for_user(&auth.user_id)
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "member_count": t.members.len(),
                "created_at": t.created_at,
            })
        })
        .collect();
    Json(json!({ "teams": teams }))
}

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
}

/// POST /api/teams — create a new team
async fn create_team(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTeamRequest>,
) -> (StatusCode, Json<Value>) {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Team name required" })),
        );
    }

    let team = Team {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        created_by: auth.user_id.clone(),
        members: vec![auth.user_id],
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let id = team.id.clone();
    let mut store = state.team_store.write().await;
    store.insert(team);

    (StatusCode::CREATED, Json(json!({ "id": id })))
}

/// GET /api/teams/{id} — get team with resolved member info
async fn get_team(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.team_store.read().await;
    let team = store.get(&id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" })))
    })?;

    if !team.members.contains(&auth.user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member" }))));
    }

    // Resolve member IDs to email/name
    let user_store = state.user_store.read().await;
    let members: Vec<Value> = team
        .members
        .iter()
        .map(|uid| {
            let user_info = user_store
                .users
                .values()
                .find(|u| u.id == *uid);
            match user_info {
                Some(u) => json!({
                    "user_id": uid,
                    "email": u.email,
                    "name": u.name,
                }),
                None => json!({ "user_id": uid }),
            }
        })
        .collect();

    Ok(Json(json!({
        "id": team.id,
        "name": team.name,
        "created_by": team.created_by,
        "members": members,
        "created_at": team.created_at,
    })))
}

#[derive(Deserialize)]
struct AddMemberRequest {
    email: String,
}

/// POST /api/teams/{id}/members — add a member by email
async fn add_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AddMemberRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let email = body.email.trim().to_lowercase();

    // Look up user by email
    let user_store = state.user_store.read().await;
    let target_user = user_store.find_by_email(&email).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" })))
    })?;
    let target_id = target_user.id.clone();
    drop(user_store);

    let mut store = state.team_store.write().await;
    let team = store.teams.get_mut(&id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" })))
    })?;

    if !team.members.contains(&auth.user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member" }))));
    }

    if team.members.contains(&target_id) {
        return Ok(Json(json!({ "ok": true, "message": "Already a member" })));
    }

    team.members.push(target_id);
    store.save();

    Ok(Json(json!({ "ok": true })))
}

/// DELETE /api/teams/{id}/members/{user_id} — remove a member
async fn remove_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, target_user_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut store = state.team_store.write().await;
    let team = store.teams.get_mut(&id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "Team not found" })))
    })?;

    if !team.members.contains(&auth.user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Not a member" }))));
    }

    team.members.retain(|m| m != &target_user_id);

    // Auto-delete empty teams
    if team.members.is_empty() {
        store.remove(&id);
        return Ok(Json(json!({ "ok": true, "team_deleted": true })));
    }

    store.save();
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_store_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut store = TeamStore::load(tmp.path());
        assert!(store.teams.is_empty());

        store.insert(Team {
            id: "t1".into(),
            name: "Test Team".into(),
            created_by: "u1".into(),
            members: vec!["u1".into(), "u2".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
        });

        let loaded = TeamStore::load(tmp.path());
        assert_eq!(loaded.teams.len(), 1);
        assert_eq!(loaded.get("t1").unwrap().name, "Test Team");
    }

    #[test]
    fn list_for_user_filters() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut store = TeamStore::load(tmp.path());

        store.insert(Team {
            id: "t1".into(),
            name: "A".into(),
            created_by: "u1".into(),
            members: vec!["u1".into(), "u2".into()],
            created_at: "".into(),
        });
        store.insert(Team {
            id: "t2".into(),
            name: "B".into(),
            created_by: "u3".into(),
            members: vec!["u3".into()],
            created_at: "".into(),
        });

        assert_eq!(store.list_for_user("u1").len(), 1);
        assert_eq!(store.list_for_user("u2").len(), 1);
        assert_eq!(store.list_for_user("u3").len(), 1);
        assert_eq!(store.list_for_user("u99").len(), 0);
    }
}
