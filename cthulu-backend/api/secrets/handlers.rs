use axum::extract::State;
use axum::Json;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::AppState;

#[derive(Serialize)]
pub struct PatStatusResponse {
    configured: bool,
}

pub async fn get_github_pat_status(
    State(state): State<AppState>,
) -> Json<PatStatusResponse> {
    let pat = state.github_pat.read().await;
    Json(PatStatusResponse {
        configured: pat.is_some(),
    })
}

#[derive(Deserialize)]
pub struct SavePatRequest {
    token: String,
}

#[derive(Serialize)]
pub struct SavePatResponse {
    ok: bool,
    username: String,
}

pub async fn save_github_pat(
    State(state): State<AppState>,
    Json(body): Json<SavePatRequest>,
) -> Result<Json<SavePatResponse>, (StatusCode, Json<serde_json::Value>)> {
    let token = body.token.trim().to_string();
    if token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Token cannot be empty"})),
        ));
    }

    // Validate the token by calling GitHub API
    let response = state
        .http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to validate GitHub PAT");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Failed to reach GitHub: {e}")})),
            )
        })?;

    if !response.status().is_success() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid GitHub token — GET /user returned non-200"})),
        ));
    }

    let user: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Failed to parse GitHub response: {e}")})),
        )
    })?;

    let username = user["login"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    // Save to secrets.json atomically
    let secrets_path = &state.secrets_path;

    // Read existing secrets or start fresh
    let mut secrets: serde_json::Value = if secrets_path.exists() {
        let content = std::fs::read_to_string(secrets_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        // Ensure parent directory exists
        if let Some(parent) = secrets_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        json!({})
    };

    secrets["github_pat"] = json!(token);

    // Atomic write: temp file + rename
    let tmp_path = secrets_path.with_extension("json.tmp");
    let json_str = serde_json::to_string_pretty(&secrets).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to serialize secrets: {e}")})),
        )
    })?;

    std::fs::write(&tmp_path, &json_str).map_err(|e| {
        tracing::error!(error = %e, "failed to write secrets temp file");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to write secrets file: {e}")})),
        )
    })?;

    std::fs::rename(&tmp_path, secrets_path).map_err(|e| {
        tracing::error!(error = %e, "failed to rename secrets temp file");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to save secrets file: {e}")})),
        )
    })?;

    // Update in-memory state
    *state.github_pat.write().await = Some(token);

    tracing::info!(username = %username, "GitHub PAT saved and validated");

    Ok(Json(SavePatResponse {
        ok: true,
        username,
    }))
}
