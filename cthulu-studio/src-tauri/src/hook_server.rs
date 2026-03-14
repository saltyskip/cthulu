use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use uuid::Uuid;

use cthulu::api::hooks::routes::{
    FileChangeEvent, PermissionDecision, PermissionRequestEvent, find_agent_id_for_session,
};
use cthulu::api::AppState;

// ---------------------------------------------------------------------------
// Wire protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct HookRequest {
    event: String,
    session_id: String,
    payload: Value,
}

/// Tools that require user approval before execution.
const TOOLS_REQUIRING_APPROVAL: &[&str] = &[
    "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash",
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start the Unix domain socket server for hook IPC.
/// Returns the socket path.
pub async fn start_hook_socket(
    state: AppState,
    app_handle: tauri::AppHandle,
) -> Result<PathBuf, String> {
    let socket_path = std::env::temp_dir().join(format!("cthulu-{}.sock", std::process::id()));

    // Remove stale socket if it exists
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)
        .map_err(|e| format!("failed to bind hook socket: {e}"))?;

    let path = socket_path.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = state.clone();
                    let app_handle = app_handle.clone();
                    tokio::spawn(handle_hook_connection(stream, state, app_handle));
                }
                Err(e) => {
                    tracing::error!(error = %e, "hook socket accept error");
                }
            }
        }
    });

    tracing::info!(path = %path.display(), "hook socket server started");
    Ok(path)
}

/// Clean up the socket file on shutdown.
pub fn cleanup_hook_socket(socket_path: &std::path::Path) {
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
        tracing::info!(path = %socket_path.display(), "hook socket cleaned up");
    }
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

async fn handle_hook_connection(
    stream: tokio::net::UnixStream,
    state: AppState,
    app_handle: tauri::AppHandle,
) {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one line of JSON from the client
    match buf_reader.read_line(&mut line).await {
        Ok(0) => return, // EOF
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "hook socket read error");
            return;
        }
    }

    let request: HookRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, raw = %line.trim(), "invalid hook request JSON");
            let err_response = json!({"error": format!("invalid JSON: {e}")});
            let _ = writer
                .write_all(format!("{}\n", err_response).as_bytes())
                .await;
            return;
        }
    };

    tracing::debug!(
        event = %request.event,
        session_id = %request.session_id,
        "hook request received"
    );

    let response = match request.event.as_str() {
        "pre-tool-use" => handle_pre_tool_use(&state, &app_handle, &request).await,
        "post-tool-use" => handle_post_tool_use(&state, &app_handle, &request).await,
        "stop" => handle_stop(&state, &app_handle, &request).await,
        other => {
            tracing::warn!(event = %other, "unknown hook event");
            json!({"error": format!("unknown event: {other}")})
        }
    };

    // Write response line and close
    let response_str = format!("{}\n", serde_json::to_string(&response).unwrap_or_default());
    if let Err(e) = writer.write_all(response_str.as_bytes()).await {
        tracing::error!(error = %e, "hook socket write error");
    }
    let _ = writer.shutdown().await;
}

// ---------------------------------------------------------------------------
// Event handlers
// ---------------------------------------------------------------------------

async fn handle_pre_tool_use(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    request: &HookRequest,
) -> Value {
    let session_id = &request.session_id;
    let tool_name = request
        .payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let tool_input = request
        .payload
        .get("tool_input")
        .cloned()
        .unwrap_or(json!({}));

    // Auto-allow tools that don't require approval
    if !TOOLS_REQUIRING_APPROVAL.contains(&tool_name.as_str()) {
        return json!({});
    }

    let agent_id = find_agent_id_for_session(state, session_id)
        .await
        .unwrap_or_default();

    let request_id = Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        agent_id = %agent_id,
        session_id = %session_id,
        tool_name = %tool_name,
        "pre-tool-use (socket): waiting for user decision"
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
        let _ = state.global_hook_tx.send(sse_data.clone());

        // Also emit Tauri event for the frontend
        let _ = app_handle.emit("hook-event", &sse_data);
        true
    } else {
        false
    };

    if !has_listener {
        tracing::info!(request_id = %request_id, "no UI connected, auto-allowing (socket)");
        let mut pending = state.pending_permissions.lock().await;
        pending.remove(&request_id);
        return json!({
            "decision": "allow",
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "No UI connected, auto-approved"
            }
        });
    }

    // Block until user responds or timeout (120s)
    let decision = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        rx,
    )
    .await
    {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => {
            tracing::warn!(request_id = %request_id, "permission channel dropped, auto-denying (socket)");
            PermissionDecision { allow: false }
        }
        Err(_) => {
            tracing::warn!(request_id = %request_id, "permission timed out after 120s, auto-denying (socket)");
            let mut pending = state.pending_permissions.lock().await;
            pending.remove(&request_id);

            // Notify frontend of timeout
            let timeout_msg = serde_json::to_string(&json!({
                "type": "permission_timeout",
                "data": { "request_id": request_id }
            }))
            .unwrap_or_default();
            let _ = state.global_hook_tx.send(timeout_msg.clone());
            let _ = app_handle.emit("hook-event", &timeout_msg);

            PermissionDecision { allow: false }
        }
    };

    if decision.allow {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user ALLOWED (socket)");
        json!({
            "decision": "allow",
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "Approved by user in Cthulu Studio"
            }
        })
    } else {
        tracing::info!(request_id = %request_id, tool_name = %tool_name, "user DENIED (socket)");
        json!({
            "decision": "deny",
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "Denied by user in Cthulu Studio"
            }
        })
    }
}

async fn handle_post_tool_use(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    request: &HookRequest,
) -> Value {
    let session_id = &request.session_id;
    let tool_name = request
        .payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    if matches!(
        tool_name.as_str(),
        "Write" | "Edit" | "MultiEdit" | "Bash" | "NotebookEdit"
    ) {
        let agent_id = find_agent_id_for_session(state, session_id)
            .await
            .unwrap_or_default();

        let tool_input = request
            .payload
            .get("tool_input")
            .cloned()
            .unwrap_or(json!({}));

        let event = FileChangeEvent {
            agent_id,
            session_id: session_id.clone(),
            tool_name: tool_name.clone(),
            tool_input,
        };

        let msg = serde_json::to_string(&json!({
            "type": "file_change",
            "data": event,
        }))
        .unwrap_or_default();

        let _ = state.global_hook_tx.send(msg.clone());
        let _ = app_handle.emit("hook-event", &msg);
    }

    json!({})
}

async fn handle_stop(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    request: &HookRequest,
) -> Value {
    let session_id = &request.session_id;
    let stop_reason = request
        .payload
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    tracing::info!(
        session_id = %session_id,
        stop_reason = ?stop_reason,
        "stop hook received (socket)"
    );

    let msg = serde_json::to_string(&json!({
        "type": "hook_stop",
        "data": {
            "session_id": session_id,
            "stop_reason": stop_reason,
        }
    }))
    .unwrap_or_default();

    let _ = state.global_hook_tx.send(msg.clone());
    let _ = app_handle.emit("hook-event", &msg);

    json!({})
}
