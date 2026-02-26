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

/// Returns whether a token is currently loaded.
pub(crate) async fn token_status(State(state): State<AppState>) -> impl IntoResponse {
    let token = state.oauth_token.read().await;
    let has_token = token.is_some();
    drop(token);
    Json(json!({ "has_token": has_token }))
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

            // Kill all live Claude processes so the next request spawns fresh ones.
            // The old processes are authenticated with the expired token — they must die.
            let killed = {
                let mut pool = state.live_processes.lock().await;
                let count = pool.len();
                for (key, mut proc) in pool.drain() {
                    tracing::info!(key = %key, "killing stale claude process on token refresh");
                    let _ = proc.child.kill().await;
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

            // Re-inject the new token into all active VMs so scheduled runs pick it up.
            // VMs store the token in ~/.bashrc; without this they keep using the expired one.
            let vm_inject_count = if let Some(vm_manager) = &state.vm_manager {
                let vm_urls: Vec<String> = {
                    let mappings = state.vm_mappings.read().await;
                    mappings.values().map(|v| v.web_terminal_url.clone()).collect()
                };
                let mut injected = 0usize;
                for url in &vm_urls {
                    if url.is_empty() {
                        continue;
                    }
                    match crate::sandbox::backends::vm_manager::inject_oauth_token_pub(url, &token, credentials_json.as_deref()).await {
                        Ok(()) => {
                            tracing::info!(vm_url = %url, "re-injected OAuth token into VM");
                            injected += 1;
                        }
                        Err(e) => {
                            tracing::warn!(vm_url = %url, error = %e, "failed to re-inject token into VM");
                        }
                    }
                }
                // suppress unused warning when vm_manager is None
                let _ = vm_manager;
                injected
            } else {
                0
            };

            tracing::info!(killed_processes = killed, vms_updated = vm_inject_count, "OAuth token refreshed successfully");
            Json(json!({
                "ok": true,
                "message": format!(
                    "Token refreshed. {} local session(s) cleared, {} VM(s) updated.",
                    killed, vm_inject_count
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

// Re-export for cross-slice access (used by flows/handlers.rs)
pub use super::repository::read_full_credentials;
