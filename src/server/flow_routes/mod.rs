mod crud;
mod interact;
mod node_chat;
mod sandbox;
mod scheduler;

use axum::routing::{delete, get, post};
use axum::Router;
use serde::Deserialize;

use super::AppState;

pub fn flow_router() -> Router<AppState> {
    Router::new()
        // Flow CRUD
        .route("/flows", get(crud::list_flows).post(crud::create_flow))
        .route(
            "/flows/{id}",
            get(crud::get_flow).put(crud::update_flow).delete(crud::delete_flow),
        )
        .route("/flows/{id}/trigger", post(crud::trigger_flow))
        .route("/flows/{id}/runs", get(crud::get_runs))
        .route("/flows/{id}/runs/live", get(crud::stream_runs))
        .route("/node-types", get(crud::get_node_types))
        // Flow-level interact
        .route("/flows/{id}/session", get(interact::get_session))
        .route("/flows/{id}/interact", post(interact::interact_flow))
        .route("/flows/{id}/interact/sessions", get(interact::list_sessions))
        .route("/flows/{id}/interact/new", post(interact::new_session))
        .route("/flows/{id}/interact/sessions/{session_id}", delete(interact::delete_session))
        .route("/flows/{id}/interact/reset", post(interact::reset_interact))
        .route("/flows/{id}/interact/stop", post(interact::stop_interact))
        // Node-level chat
        .route("/flows/{id}/nodes/{node_id}/session", get(node_chat::get_node_session))
        .route("/flows/{id}/nodes/{node_id}/interact", post(node_chat::interact_node))
        .route("/flows/{id}/nodes/{node_id}/interact/sessions", get(node_chat::list_node_sessions))
        .route("/flows/{id}/nodes/{node_id}/interact/new", post(node_chat::new_node_session))
        .route("/flows/{id}/nodes/{node_id}/interact/sessions/{session_id}", delete(node_chat::delete_node_session))
        .route("/flows/{id}/nodes/{node_id}/interact/stop", post(node_chat::stop_node_interact))
        // Scheduler / cron
        .route("/flows/{id}/schedule", get(scheduler::get_schedule))
        .route("/scheduler/status", get(scheduler::scheduler_status))
        .route("/validate/cron", post(scheduler::validate_cron))
        // Sandbox
        .route("/sandbox/info", get(sandbox::sandbox_info))
        .route("/sandbox/list", get(sandbox::sandbox_list))
        // VM Manager
        .route("/sandbox/vm/{flow_id}", get(sandbox::get_flow_vm).post(sandbox::create_flow_vm).delete(sandbox::delete_flow_vm))
}

// ---------------------------------------------------------------------------
// Shared types and helpers used across sub-modules
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct InteractRequest {
    pub prompt: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct StopRequest {
    pub session_id: Option<String>,
}

/// Resolve working_dir from flow config or fallback to cwd.
pub(super) fn resolve_working_dir(session_info: &Option<crate::flows::runner::SessionInfo>) -> String {
    session_info
        .as_ref()
        .map(|s| s.working_dir.clone())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| ".".into())
                .to_string_lossy()
                .to_string()
        })
}

/// Truncate a string to ~80 chars for use as a summary, breaking at word boundary.
/// Uses char-safe indexing to avoid panics on multi-byte UTF-8 characters.
pub(super) fn make_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(80).collect();
    let boundary = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}...", &truncated[..boundary])
}

/// Best-effort process termination, platform-specific.
pub(super) fn kill_pid(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .spawn();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .spawn();
    }
}

/// Build the path for node attachment files.
pub(super) fn attachments_path(data_dir: &std::path::Path, flow_id: &str, node_id: &str) -> std::path::PathBuf {
    data_dir.join("attachments").join(flow_id).join(node_id)
}
