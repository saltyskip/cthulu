use serde_json::{json, Value};

use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// Token status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn token_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let token = state.oauth_token.read().await;
    let has_token = token.is_some();
    drop(token);

    // Try to read richer info from the Keychain blob
    let creds = cthulu::api::auth::handlers::read_full_credentials()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());

    let oauth = creds.as_ref().and_then(|v| v.get("claudeAiOauth"));

    let expires_at = oauth
        .and_then(|o| o.get("expiresAt"))
        .and_then(|v| v.as_i64())
        .map(|ms| {
            let secs = ms / 1000;
            let dt =
                chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0).unwrap_or_default();
            dt.to_rfc3339()
        });

    let is_expired = oauth
        .and_then(|o| o.get("expiresAt"))
        .and_then(|v| v.as_i64())
        .map(|ms| {
            let now_ms = chrono::Utc::now().timestamp_millis();
            ms < now_ms
        })
        .unwrap_or(false);

    let subscription_type = oauth
        .and_then(|o| o.get("subscriptionType"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let rate_limit_tier = oauth
        .and_then(|o| o.get("rateLimitTier"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let status = if !has_token {
        "missing"
    } else if is_expired {
        "expired"
    } else {
        "valid"
    };

    Ok(json!({
        "has_token": has_token,
        "status": status,
        "expires_at": expires_at,
        "is_expired": is_expired,
        "subscription_type": subscription_type,
        "rate_limit_tier": rate_limit_tier,
    }))
}

// ---------------------------------------------------------------------------
// Refresh token
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn refresh_token(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let new_token = cthulu::api::auth::repository::read_oauth_token();

    match new_token {
        Some(token) => {
            // Update in-memory token
            {
                let mut guard = state.oauth_token.write().await;
                *guard = Some(token.clone());
            }

            // Kill all live Claude processes so the next request spawns fresh ones
            let killed = {
                let mut pool = state.live_processes.lock().await;
                let count = pool.len();
                for (key, mut proc) in pool.drain() {
                    tracing::info!(key = %key, "killing stale claude process on token refresh");
                    let _ = proc.child.kill().await;
                }
                count
            };

            // Clear busy flag on all sessions
            {
                let mut sessions = state.interact_sessions.write().await;
                for flow_sessions in sessions.values_mut() {
                    for session in &mut flow_sessions.sessions {
                        session.busy = false;
                        session.active_pid = None;
                    }
                }
            }

            Ok(json!({
                "ok": true,
                "message": format!(
                    "Token refreshed. {} local session(s) cleared.",
                    killed
                ),
            }))
        }
        None => Ok(json!({
            "ok": false,
            "message": "No token found in Keychain or CLAUDE_CODE_OAUTH_TOKEN env. Run `claude` in your terminal to re-authenticate, then try again.",
        })),
    }
}
