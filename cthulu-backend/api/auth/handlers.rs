/// Auth endpoints for OAuth token management.
///
/// GET  /api/auth/token-status   — check if a token is loaded
/// POST /api/auth/refresh-token  — re-read token from Keychain / env, update in-memory,
///                                  and re-inject into all active VMs
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::api::AppState;

use super::repository;

/// Returns whether a token is currently loaded, plus expiry and account info
/// extracted from the macOS Keychain credentials blob.
pub(crate) async fn token_status(State(state): State<AppState>) -> impl IntoResponse {
    let token = state.oauth_token.read().await;
    let has_token = token.is_some();
    drop(token);

    // Try to read richer info from the Keychain blob
    let creds = repository::read_full_credentials()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());

    let oauth = creds.as_ref().and_then(|v| v.get("claudeAiOauth"));

    let expires_at = oauth
        .and_then(|o| o.get("expiresAt"))
        .and_then(|v| v.as_i64())
        .map(|ms| {
            // expiresAt is milliseconds since epoch
            let secs = ms / 1000;
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
                .unwrap_or_default();
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

    Json(json!({
        "has_token": has_token,
        "status": status,
        "expires_at": expires_at,
        "is_expired": is_expired,
        "subscription_type": subscription_type,
        "rate_limit_tier": rate_limit_tier
    }))
}

/// Re-reads the OAuth token from the macOS Keychain or CLAUDE_CODE_OAUTH_TOKEN env,
/// updates the in-memory token, kills all stale live Claude processes (so the next
/// message spawns a fresh process with the new token), and returns the result.
pub(crate) async fn refresh_token(State(state): State<AppState>) -> impl IntoResponse {
    let new_token = repository::read_oauth_token();
    let credentials_json = repository::read_full_credentials();

    match new_token {
        Some(token) => {
            // Update in-memory token
            {
                let mut guard = state.oauth_token.write().await;
                *guard = Some(token.clone());
            }

            // Disconnect all SDK sessions so the next request creates fresh ones.
            // The old sessions are authenticated with the expired token — they must be replaced.
            let disconnected = {
                let mut pool = state.sdk_sessions.lock().await;
                let count = pool.len();
                for (key, mut session) in pool.drain() {
                    tracing::info!(key = %key, "disconnecting stale SDK session on token refresh");
                    let _ = session.disconnect().await;
                }
                count
            };

            // Also clear busy flag on all sessions so users can send again immediately
            {
                let mut sessions = state.interact_sessions.write().await;
                for flow_sessions in sessions.values_mut() {
                    for session in &mut flow_sessions.sessions {
                        session.busy = false;
                        session.active_pid = None;
                    }
                }
            }

            tracing::info!(disconnected_sessions = disconnected, "OAuth token refreshed successfully");
            Json(json!({
                "ok": true,
                "message": format!(
                    "Token refreshed. {} session(s) cleared.",
                    disconnected
                )
            }))
        }
        None => {
            tracing::warn!("OAuth token refresh failed — no token found in Keychain or env");
            Json(json!({
                "ok": false,
                "message": "No token found in Keychain or CLAUDE_CODE_OAUTH_TOKEN env. Run `claude` in your terminal to re-authenticate, then try again."
            }))
        }
    }
}

/// POST /api/auth/refresh-jwt — issue a new JWT with extended expiry.
/// For cloud deployments where the frontend needs to refresh tokens without
/// re-authenticating (prevents hard 24h logout).
pub(crate) async fn refresh_jwt(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Extract existing JWT from Authorization header
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (
            hyper::StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing Authorization header" })),
        );
    };

    // Validate existing token
    let decoding_key = jsonwebtoken::DecodingKey::from_secret(state.jwt_secret.as_bytes());
    let mut validation = jsonwebtoken::Validation::default();
    validation.validate_exp = false; // Allow expired tokens to be refreshed

    let claims = match jsonwebtoken::decode::<serde_json::Value>(token, &decoding_key, &validation) {
        Ok(data) => data.claims,
        Err(_) => {
            return (
                hyper::StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid token" })),
            );
        }
    };

    // Issue a new token with fresh expiry (24h)
    let sub = claims.get("sub").and_then(|v| v.as_str()).unwrap_or("unknown");
    let now = chrono::Utc::now().timestamp() as usize;
    let new_claims = json!({
        "sub": sub,
        "iat": now,
        "exp": now + 86400, // 24 hours
    });

    let encoding_key = jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes());
    match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &new_claims,
        &encoding_key,
    ) {
        Ok(new_token) => (
            hyper::StatusCode::OK,
            Json(json!({
                "token": new_token,
                "expires_in": 86400,
            })),
        ),
        Err(e) => (
            hyper::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to issue token: {e}") })),
        ),
    }
}

// Re-export for cross-slice access (used by flows/handlers.rs)
pub use super::repository::read_full_credentials;
