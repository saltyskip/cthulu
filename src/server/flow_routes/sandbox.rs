use axum::extract::{Path, State};
use axum::Json;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use super::super::AppState;

/// GET /api/sandbox/info — provider info and capabilities.
pub(crate) async fn sandbox_info(State(state): State<AppState>) -> Json<Value> {
    let info = state.sandbox_provider.info();
    Json(json!({
        "provider": format!("{:?}", info.kind),
        "supports_persistent_state": info.supports_persistent_state,
        "supports_checkpoint": info.supports_checkpoint,
        "supports_public_http": info.supports_public_http,
        "supports_sleep_resume": info.supports_sleep_resume,
    }))
}

/// GET /api/sandbox/list — list active sandboxes.
pub(crate) async fn sandbox_list(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    match state.sandbox_provider.list().await {
        Ok(sandboxes) => {
            let items: Vec<Value> = sandboxes
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "backend": format!("{:?}", s.backend),
                        "status": format!("{:?}", s.status),
                        "workspace_id": s.workspace_id,
                    })
                })
                .collect();
            Ok(Json(json!({ "sandboxes": items })))
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list sandboxes");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── VM Manager endpoints ────────────────────────────────────────────
//
// These proxy requests to the VM Manager API. Only available when the
// sandbox provider is VmManager.

/// Helper to get the VmManagerProvider from AppState.
/// Returns 503 Service Unavailable with a descriptive JSON body if VM Manager is not configured.
fn require_vm_manager(
    state: &AppState,
) -> Result<&crate::sandbox::backends::vm_manager::VmManagerProvider, (StatusCode, Json<Value>)> {
    state.vm_manager.as_deref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "VM Manager not configured. Set VM_MANAGER_URL in .env" })),
    ))
}

#[derive(Deserialize)]
pub(crate) struct VmCreateBody {
    pub tier: Option<String>,
    pub api_key: Option<String>,
}

/// GET /api/sandbox/vm/{flow_id}/{node_id} — get VM info for an executor node.
pub(crate) async fn get_node_vm(
    State(state): State<AppState>,
    Path((flow_id, node_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let provider = require_vm_manager(&state)?;

    match provider.get_node_vm(&flow_id, &node_id).await {
        Some(vm) => Ok(Json(json!({
            "vm_id": vm.vm_id,
            "tier": vm.tier,
            "guest_ip": vm.guest_ip,
            "ssh_port": vm.ssh_port,
            "web_port": vm.web_port,
            "ssh_command": vm.ssh_command,
            "web_terminal": vm.web_terminal,
            "pid": vm.pid,
        }))),
        None => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "No VM exists for this node" })))),
    }
}

/// POST /api/sandbox/vm/{flow_id}/{node_id} — create (or get existing) VM for an executor node.
pub(crate) async fn create_node_vm(
    State(state): State<AppState>,
    Path((flow_id, node_id)): Path<(String, String)>,
    Json(body): Json<VmCreateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let provider = require_vm_manager(&state)?;

    match provider
        .get_or_create_vm(
            &flow_id,
            &node_id,
            body.tier.as_deref(),
            body.api_key.as_deref(),
        )
        .await
    {
        Ok(vm) => Ok(Json(json!({
            "vm_id": vm.vm_id,
            "tier": vm.tier,
            "guest_ip": vm.guest_ip,
            "ssh_port": vm.ssh_port,
            "web_port": vm.web_port,
            "ssh_command": vm.ssh_command,
            "web_terminal": vm.web_terminal,
            "pid": vm.pid,
        }))),
        Err(e) => {
            tracing::error!(flow_id = %flow_id, node_id = %node_id, error = %e, "failed to create VM");
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
        }
    }
}

/// DELETE /api/sandbox/vm/{flow_id}/{node_id} — destroy VM for an executor node.
pub(crate) async fn delete_node_vm(
    State(state): State<AppState>,
    Path((flow_id, node_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let provider = require_vm_manager(&state)?;

    match provider.destroy_node_vm(&flow_id, &node_id).await {
        Ok(Some(vm_id)) => {
            tracing::info!(flow_id = %flow_id, node_id = %node_id, vm_id = vm_id, "VM destroyed");
            Ok(Json(json!({ "status": "deleted", "vm_id": vm_id })))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "No VM exists for this node" })))),
        Err(e) => {
            tracing::error!(flow_id = %flow_id, node_id = %node_id, error = %e, "failed to destroy VM");
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
        }
    }
}
