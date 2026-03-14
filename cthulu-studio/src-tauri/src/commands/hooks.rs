use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;
use cthulu::api::hooks::routes::PermissionDecision;

// ---------------------------------------------------------------------------
// Permission response
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PermissionResponseRequest {
    pub request_id: String,
    /// "allow" or "deny"
    pub decision: String,
}

#[tauri::command]
pub async fn permission_response(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: PermissionResponseRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let allow = request.decision == "allow";

    tracing::info!(
        request_id = %request.request_id,
        decision = %request.decision,
        "permission response from Tauri frontend"
    );

    let mut pending = state.pending_permissions.lock().await;
    if let Some((tx, _event)) = pending.remove(&request.request_id) {
        let _ = tx.send(PermissionDecision { allow });
        Ok(json!({ "ok": true }))
    } else {
        Ok(json!({ "ok": false, "error": "no pending permission found" }))
    }
}

// ---------------------------------------------------------------------------
// List pending permissions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_pending_permissions(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pending = state.pending_permissions.lock().await;
    let requests: Vec<Value> = pending
        .values()
        .map(|(_tx, event)| serde_json::to_value(event).unwrap_or_default())
        .collect();

    Ok(json!({ "pending": requests }))
}
