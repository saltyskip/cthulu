use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::Stream;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
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
// Permission types
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

/// Permission request metadata — stored alongside the oneshot sender
/// so we can return pending requests on reconnection.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionRequestEvent {
    pub request_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub tool_input: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileChangeEvent {
    pub agent_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub tool_input: Value,
}

const TOOLS_REQUIRING_APPROVAL: &[&str] = &[
    "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash",
];

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// PreToolUse hook — the primary permission gate.
///
/// Broadcasts to a single global SSE stream so the frontend receives
/// permission requests regardless of which session tab is active.
pub async fn pre_tool_use(
    State(state): State<AppState>,
    Query(query): Query<HookQuery>,
    Json(body): Json<PreToolUsePayload>,
) -> Result<Json<Value>, StatusCode> {
    let session_id = query.session_id.unwrap_or_default();
    let tool_name = body.tool_name.unwrap_or_else(|| "unknown".to_string());
    let tool_input = body.tool_input.unwrap_or(json!({}));

    if !TOOLS_REQUIRING_APPROVAL.contains(&tool_name.as_str()) {
        return Ok(Json(json!({})));
    }

    let agent_id = find_agent_id_for_session(&state, &session_id)
        .await
        .unwrap_or_default();

    let request_id = Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        agent_id = %agent_id,
        session_id = %session_id,
        tool_name = %tool_name,
        "pre-tool-use: waiting for user decision"
    );

    let event = PermissionRequestEvent {
        request_id: request_id.clone(),
        agent_id: agent_id.clone(),
        session_id: session_id.clone(),
        tool_name: tool_name.clone(),
        tool_input: tool_input.clone(),
    };

    // Create oneshot channel and store with metadata
    let (tx, rx) = tokio::sync::oneshot::channel::<PermissionDecision>();
    {
        let mut pending = state.pending_permissions.lock().await;
        pending.insert(request_id.clone(), (tx, event.clone()));
    }

    // Broadcast to the global hook stream
    let has_listener = if state.global_hook_tx.receiver_count() > 0 {
        let sse_data = serde_json::to_string(&json!({
            "type": "permission_request",
            "data": event,
        }))
        .unwrap_or_default();
        let _ = state.global_hook_tx.send(sse_data);
        true
    } else {
        false
    };

    if !has_listener {
        tracing::info!(request_id = %request_id, "no UI connected, auto-allowing");
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

    // Block until user responds or timeout
    let decision = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        rx,
    )
    .await
    {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => {
            // Oneshot dropped (agent killed, session cleaned up) — deny
            tracing::warn!(request_id = %request_id, "permission channel dropped, auto-denying");
            PermissionDecision { allow: false }
        }
        Err(_) => {
            tracing::warn!(request_id = %request_id, "permission timed out after 120s, auto-denying");
            let mut pending = state.pending_permissions.lock().await;
            pending.remove(&request_id);

            // Notify frontend of timeout
            let _ = state.global_hook_tx.send(
                serde_json::to_string(&json!({
                    "type": "permission_timeout",
                    "data": { "request_id": request_id }
                }))
                .unwrap_or_default(),
            );

            PermissionDecision { allow: false }
        }
    };

    if decision.allow {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user ALLOWED");
        Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "Approved by user in Cthulu Studio"
            }
        })))
    } else {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user DENIED");
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
    if let Some((tx, _event)) = pending.remove(&body.request_id) {
        let _ = tx.send(PermissionDecision { allow });
        Ok(Json(json!({ "ok": true })))
    } else {
        tracing::warn!(request_id = %body.request_id, "no pending permission found");
        Ok(Json(json!({ "ok": false, "error": "no pending permission found" })))
    }
}

/// Return all currently pending permission requests (for reconnection).
pub async fn list_pending(
    State(state): State<AppState>,
) -> Json<Value> {
    let pending = state.pending_permissions.lock().await;
    let requests: Vec<&PermissionRequestEvent> = pending.values()
        .map(|(_tx, event)| event)
        .collect();
    Json(json!({ "pending": requests }))
}

/// PostToolUse — broadcast file changes to the global hook stream.
pub async fn post_tool_use(
    State(state): State<AppState>,
    Query(query): Query<HookQuery>,
    Json(body): Json<PostToolUsePayload>,
) -> Json<Value> {
    let session_id = query.session_id.unwrap_or_default();
    let tool_name = body.tool_name.unwrap_or_default();

    if matches!(
        tool_name.as_str(),
        "Write" | "Edit" | "MultiEdit" | "Bash" | "NotebookEdit"
    ) {
        let agent_id = find_agent_id_for_session(&state, &session_id)
            .await
            .unwrap_or_default();

        let event = FileChangeEvent {
            agent_id,
            session_id: session_id.clone(),
            tool_name: tool_name.clone(),
            tool_input: body.tool_input.unwrap_or(json!({})),
        };

        let _ = state.global_hook_tx.send(
            serde_json::to_string(&json!({
                "type": "file_change",
                "data": event,
            }))
            .unwrap_or_default(),
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

    let _ = state.global_hook_tx.send(
        serde_json::to_string(&json!({
            "type": "hook_stop",
            "data": {
                "session_id": session_id,
                "stop_reason": body.stop_reason,
            }
        }))
        .unwrap_or_default(),
    );

    Json(json!({}))
}

/// Global hook event SSE stream — frontend subscribes once at App mount.
/// Receives permission requests, file changes, and stop events from ALL sessions.
pub async fn global_hook_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.global_hook_tx.subscribe();

    tracing::info!("global hook stream connected");

    let stream = async_stream::stream! {
        let mut rx = rx;

        yield Ok(Event::default().event("connected").data("{}"));

        loop {
            match rx.recv().await {
                Ok(data) => {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
                        let event_type = parsed.get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("hook");
                        yield Ok(Event::default().event(event_type).data(data));
                    } else {
                        yield Ok(Event::default().event("hook").data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "global hook stream lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the agent_id that owns a given Claude session_id.
pub async fn find_agent_id_for_session(state: &AppState, session_id: &str) -> Option<String> {
    if session_id.is_empty() {
        return None;
    }

    let all_sessions = state.interact_sessions.read().await;
    for (key, flow_sessions) in all_sessions.iter() {
        for session in &flow_sessions.sessions {
            if session.session_id == session_id {
                if let Some(agent_id) = key.strip_prefix("agent::") {
                    return Some(agent_id.to_string());
                }
            }
        }
    }
    None
}
