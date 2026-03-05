use axum::extract::{Query, State};
use axum::Json;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct HookQuery {
    pub session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Hook payload types (from Claude Code)
// ---------------------------------------------------------------------------

/// PreToolUse payload — Claude Code sends this before executing a tool.
/// This is the PRIMARY permission gate (PermissionRequest doesn't fire in stream-json mode).
#[derive(Debug, Deserialize)]
pub struct PreToolUsePayload {
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostToolUsePayload {
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub tool_response: Option<Value>,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StopPayload {
    pub session_id: Option<String>,
    pub stop_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Permission decision (frontend -> backend)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PermissionResponseBody {
    pub request_id: String,
    /// "allow" or "deny"
    pub decision: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionDecision {
    pub allow: bool,
}

// ---------------------------------------------------------------------------
// SSE event types broadcast to frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PermissionRequestEvent {
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileChangeEvent {
    pub tool_name: String,
    pub tool_input: Value,
    pub session_id: String,
}

// Tools that require user approval before executing
const TOOLS_REQUIRING_APPROVAL: &[&str] = &[
    "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash",
];

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// PreToolUse hook — the primary permission gate.
///
/// Claude Code POSTs here BEFORE executing a tool. In stream-json mode,
/// PermissionRequest hooks don't fire, so this is where we intercept.
///
/// For tools requiring approval (Write, Edit, Bash, etc.), we:
/// 1. Broadcast a permission_request SSE event to the frontend
/// 2. Block (up to 120s) waiting for the user to Allow/Deny
/// 3. Return hookSpecificOutput with permissionDecision
///
/// For read-only tools (Read, Grep, Glob, etc.), we auto-allow.
pub async fn pre_tool_use(
    State(state): State<AppState>,
    Query(query): Query<HookQuery>,
    Json(body): Json<PreToolUsePayload>,
) -> Result<Json<Value>, StatusCode> {
    let session_id = query.session_id.unwrap_or_default();
    let tool_name = body.tool_name.unwrap_or_else(|| "unknown".to_string());
    let tool_input = body.tool_input.unwrap_or(json!({}));

    // Auto-allow tools that don't need permission
    if !TOOLS_REQUIRING_APPROVAL.contains(&tool_name.as_str()) {
        tracing::debug!(
            session_id = %session_id,
            tool_name = %tool_name,
            "pre-tool-use: auto-allowing read-only tool"
        );
        return Ok(Json(json!({})));
    }

    let request_id = Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        session_id = %session_id,
        tool_name = %tool_name,
        "pre-tool-use: tool requires approval, waiting for user decision"
    );

    // Create oneshot channel for the user's decision
    let (tx, rx) = tokio::sync::oneshot::channel::<PermissionDecision>();

    // Store the sender so the permission-response endpoint can resolve it
    {
        let mut pending = state.pending_permissions.lock().await;
        pending.insert(request_id.clone(), tx);
    }

    // Broadcast permission_request SSE event to frontend via persistent hook stream
    let event = PermissionRequestEvent {
        request_id: request_id.clone(),
        tool_name: tool_name.clone(),
        tool_input: tool_input.clone(),
        session_id: session_id.clone(),
    };

    let hook_key = find_hook_key_for_session(&state, &session_id).await;
    if let Some(ref key) = hook_key {
        let streams = state.hook_streams.lock().await;
        if let Some(tx) = streams.get(key) {
            let sse_data = serde_json::to_string(&json!({
                "type": "permission_request",
                "data": event,
            }))
            .unwrap_or_default();
            let _ = tx.send(sse_data);
            tracing::info!(key = %key, request_id = %request_id, "broadcast permission_request to hook stream");
        } else {
            tracing::warn!(key = %key, "no hook stream subscriber — auto-allowing");
            // No UI connected, auto-allow to avoid blocking forever
            let mut pending = state.pending_permissions.lock().await;
            pending.remove(&request_id);
            return Ok(Json(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "No UI connected, auto-approved"
                }
            })));
        }
    } else {
        tracing::warn!(session_id = %session_id, "could not find hook key for session — auto-allowing");
        let mut pending = state.pending_permissions.lock().await;
        pending.remove(&request_id);
        return Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "Session not found, auto-approved"
            }
        })));
    }

    // Wait for user response with timeout
    let decision = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        rx,
    )
    .await
    {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => {
            tracing::warn!(request_id = %request_id, "permission channel dropped, auto-denying");
            PermissionDecision { allow: false }
        }
        Err(_) => {
            tracing::warn!(request_id = %request_id, "permission request timed out after 120s, auto-denying");
            let mut pending = state.pending_permissions.lock().await;
            pending.remove(&request_id);

            // Notify frontend of timeout
            if let Some(ref key) = hook_key {
                let streams = state.hook_streams.lock().await;
                if let Some(tx) = streams.get(key) {
                    let sse_data = serde_json::to_string(&json!({
                        "type": "permission_timeout",
                        "data": { "request_id": request_id }
                    }))
                    .unwrap_or_default();
                    let _ = tx.send(sse_data);
                }
            }

            PermissionDecision { allow: false }
        }
    };

    // Response format per https://code.claude.com/docs/en/hooks#pretooluse-decision-control
    if decision.allow {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user ALLOWED tool");
        Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "Approved by user in Cthulu Studio"
            }
        })))
    } else {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user DENIED tool");
        Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "Denied by user in Cthulu Studio"
            }
        })))
    }
}

/// Frontend POSTs here to resolve a pending permission request.
pub async fn permission_response(
    State(state): State<AppState>,
    Json(body): Json<PermissionResponseBody>,
) -> Result<Json<Value>, StatusCode> {
    let allow = body.decision == "allow";

    tracing::info!(
        request_id = %body.request_id,
        decision = %body.decision,
        "permission response from frontend"
    );

    let mut pending = state.pending_permissions.lock().await;
    if let Some(tx) = pending.remove(&body.request_id) {
        let _ = tx.send(PermissionDecision { allow });
        Ok(Json(json!({ "ok": true })))
    } else {
        tracing::warn!(request_id = %body.request_id, "no pending permission found (expired or already resolved)");
        Ok(Json(json!({ "ok": false, "error": "no pending permission found" })))
    }
}

/// Claude Code POSTs here after using a tool. Broadcast file changes to frontend.
pub async fn post_tool_use(
    State(state): State<AppState>,
    Query(query): Query<HookQuery>,
    Json(body): Json<PostToolUsePayload>,
) -> Json<Value> {
    let session_id = query.session_id.unwrap_or_default();
    let tool_name = body.tool_name.unwrap_or_default();

    let is_file_op = matches!(
        tool_name.as_str(),
        "Write" | "Edit" | "MultiEdit" | "Bash" | "NotebookEdit"
    );

    if is_file_op {
        let event = FileChangeEvent {
            tool_name: tool_name.clone(),
            tool_input: body.tool_input.unwrap_or(json!({})),
            session_id: session_id.clone(),
        };

        let hook_key = find_hook_key_for_session(&state, &session_id).await;
        if let Some(ref key) = hook_key {
            let streams = state.hook_streams.lock().await;
            if let Some(tx) = streams.get(key) {
                let sse_data = serde_json::to_string(&json!({
                    "type": "file_change",
                    "data": event,
                }))
                .unwrap_or_default();
                let _ = tx.send(sse_data);
            }
        }

        tracing::debug!(
            session_id = %session_id,
            tool_name = %tool_name,
            "post-tool-use: file change broadcast"
        );
    }

    Json(json!({}))
}

/// Claude Code POSTs here when the session stops.
pub async fn stop(
    State(state): State<AppState>,
    Query(query): Query<HookQuery>,
    Json(body): Json<StopPayload>,
) -> Json<Value> {
    let session_id = query.session_id.unwrap_or_default();

    tracing::info!(
        session_id = %session_id,
        stop_reason = ?body.stop_reason,
        "stop hook received"
    );

    let hook_key = find_hook_key_for_session(&state, &session_id).await;
    if let Some(ref key) = hook_key {
        let streams = state.hook_streams.lock().await;
        if let Some(tx) = streams.get(key) {
            let sse_data = serde_json::to_string(&json!({
                "type": "hook_stop",
                "data": {
                    "session_id": session_id,
                    "stop_reason": body.stop_reason,
                }
            }))
            .unwrap_or_default();
            let _ = tx.send(sse_data);
        }
    }

    Json(json!({}))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the hook stream key (agent::{id}::session::{sid}) for a given Claude session_id.
async fn find_hook_key_for_session(state: &AppState, session_id: &str) -> Option<String> {
    if session_id.is_empty() {
        return None;
    }

    let all_sessions = state.interact_sessions.read().await;
    for (key, flow_sessions) in all_sessions.iter() {
        for session in &flow_sessions.sessions {
            if session.session_id == session_id {
                if let Some(agent_id) = key.strip_prefix("agent::") {
                    return Some(format!("agent::{agent_id}::session::{session_id}"));
                }
            }
        }
    }
    None
}
