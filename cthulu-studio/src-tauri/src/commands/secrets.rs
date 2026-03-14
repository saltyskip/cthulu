use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// Shared helper: save a single key to secrets.json atomically
// ---------------------------------------------------------------------------

fn save_secret_field(
    secrets_path: &std::path::Path,
    key: &str,
    value: &str,
) -> Result<Value, String> {
    if value.is_empty() {
        return Err(format!("{key} cannot be empty"));
    }

    let mut secrets: Value = if secrets_path.exists() {
        let content = std::fs::read_to_string(secrets_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        if let Some(parent) = secrets_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        json!({})
    };

    secrets[key] = json!(value);

    let tmp_path = secrets_path.with_extension("json.tmp");
    let json_str = serde_json::to_string_pretty(&secrets)
        .map_err(|e| format!("Failed to serialize secrets: {e}"))?;

    std::fs::write(&tmp_path, &json_str)
        .map_err(|e| format!("Failed to write secrets file: {e}"))?;

    std::fs::rename(&tmp_path, secrets_path)
        .map_err(|e| format!("Failed to save secrets file: {e}"))?;

    Ok(json!({ "ok": true }))
}

/// Save multiple keys to secrets.json in one atomic write.
fn save_secret_fields(
    secrets_path: &std::path::Path,
    fields: &[(&str, &str)],
) -> Result<Value, String> {
    for (key, value) in fields {
        if value.is_empty() {
            return Err(format!("{key} cannot be empty"));
        }
    }

    let mut secrets: Value = if secrets_path.exists() {
        let content = std::fs::read_to_string(secrets_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        if let Some(parent) = secrets_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        json!({})
    };

    for (key, value) in fields {
        secrets[*key] = json!(value);
    }

    let tmp_path = secrets_path.with_extension("json.tmp");
    let json_str = serde_json::to_string_pretty(&secrets)
        .map_err(|e| format!("Failed to serialize secrets: {e}"))?;

    std::fs::write(&tmp_path, &json_str)
        .map_err(|e| format!("Failed to write secrets file: {e}"))?;

    std::fs::rename(&tmp_path, secrets_path)
        .map_err(|e| format!("Failed to save secrets file: {e}"))?;

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Get GitHub PAT status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_github_pat_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = state.github_pat.read().await;
    Ok(json!({ "configured": pat.is_some() }))
}

// ---------------------------------------------------------------------------
// Save GitHub PAT
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SavePatRequest {
    token: String,
}

#[tauri::command]
pub async fn save_github_pat(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SavePatRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let token = request.token.trim().to_string();
    if token.is_empty() {
        return Err("Token cannot be empty".to_string());
    }

    let response = state
        .http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {e}"))?;

    if !response.status().is_success() {
        return Err("Invalid GitHub token — GET /user returned non-200".to_string());
    }

    let user: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {e}"))?;

    let username = user["login"].as_str().unwrap_or("unknown").to_string();

    save_secret_field(&state.secrets_path, "github_pat", &token)?;
    *state.github_pat.write().await = Some(token);

    Ok(json!({ "ok": true, "username": username }))
}

// ---------------------------------------------------------------------------
// Check setup status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn check_setup_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = state.github_pat.read().await;
    let has_pat = pat.is_some();
    drop(pat);

    let token = state.oauth_token.read().await;
    let has_oauth = token.is_some();
    drop(token);

    let secrets: Value = std::fs::read_to_string(&state.secrets_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| json!({}));

    let has_key = |key: &str| -> bool {
        secrets[key].as_str().map(|s| !s.is_empty()).unwrap_or(false)
    };

    let has_anthropic = has_key("anthropic_api_key");
    let has_slack = has_key("slack_webhook_url");

    Ok(json!({
        "setup_complete": has_pat && has_anthropic && has_slack,
        "github_pat_configured": has_pat,
        "claude_oauth_configured": has_oauth,
        "anthropic_api_key_configured": has_anthropic,
        "openai_api_key_configured": has_key("openai_api_key"),
        "slack_webhook_configured": has_slack,
        "notion_configured": has_key("notion_token") && has_key("notion_database_id"),
        "telegram_configured": has_key("telegram_bot_token") && has_key("telegram_chat_id"),
    }))
}

// ---------------------------------------------------------------------------
// Save Anthropic key
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveAnthropicKeyRequest {
    key: String,
}

#[tauri::command]
pub async fn save_anthropic_key(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SaveAnthropicKeyRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    save_secret_field(&state.secrets_path, "anthropic_api_key", request.key.trim())
}

// ---------------------------------------------------------------------------
// Save OpenAI key
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveOpenaiKeyRequest {
    key: String,
}

#[tauri::command]
pub async fn save_openai_key(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SaveOpenaiKeyRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    save_secret_field(&state.secrets_path, "openai_api_key", request.key.trim())
}

// ---------------------------------------------------------------------------
// Save Slack webhook URL
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveSlackWebhookRequest {
    url: String,
}

#[tauri::command]
pub async fn save_slack_webhook(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SaveSlackWebhookRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    save_secret_field(&state.secrets_path, "slack_webhook_url", request.url.trim())
}

// ---------------------------------------------------------------------------
// Save Notion credentials (token + database_id)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveNotionCredentialsRequest {
    token: String,
    database_id: String,
}

#[tauri::command]
pub async fn save_notion_credentials(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SaveNotionCredentialsRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    save_secret_fields(
        &state.secrets_path,
        &[
            ("notion_token", request.token.trim()),
            ("notion_database_id", request.database_id.trim()),
        ],
    )
}

// ---------------------------------------------------------------------------
// Save Telegram credentials (bot_token + chat_id)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveTelegramCredentialsRequest {
    bot_token: String,
    chat_id: String,
}

#[tauri::command]
pub async fn save_telegram_credentials(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SaveTelegramCredentialsRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    save_secret_fields(
        &state.secrets_path,
        &[
            ("telegram_bot_token", request.bot_token.trim()),
            ("telegram_chat_id", request.chat_id.trim()),
        ],
    )
}
