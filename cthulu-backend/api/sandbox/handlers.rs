use axum::extract::{Path, State};
use axum::Json;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;

use super::repository::{require_vm_manager, SandboxRepository};

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

#[derive(Deserialize)]
pub(crate) struct VmCreateBody {
    pub tier: Option<String>,
    pub api_key: Option<String>,
}

/// GET /api/sandbox/vm/{flow_id}/{node_id} — get VM info for an executor node.
///
/// Lookup order:
/// 1. In-memory `node_vms` map (fast path, populated during this server session).
/// 2. `vm_mappings` (persisted in sessions.yaml) — used after a server restart.
///    The vm_id stored there is used to query the VM Manager API; if the VM is
///    still alive its data is seeded back into `node_vms` for future fast-path hits.
pub(crate) async fn get_node_vm(
    State(state): State<AppState>,
    Path((flow_id, node_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let provider = require_vm_manager(&state)?;
    let repo = SandboxRepository::new(state.vm_mappings.clone());

    // Fast path: in-memory node_vms map
    if let Some(vm) = provider.get_node_vm(&flow_id, &node_id).await {
        return Ok(Json(json!({
            "vm_id": vm.vm_id,
            "tier": vm.tier,
            "guest_ip": vm.guest_ip,
            "ssh_port": vm.ssh_port,
            "web_port": vm.web_port,
            "ssh_command": vm.ssh_command,
            "web_terminal": vm.web_terminal,
            "pid": vm.pid,
        })));
    }

    // Fallback: check vm_mappings (populated from sessions.yaml on startup)
    let key = format!("{}::{}", flow_id, node_id);
    let vm_id_opt = repo.get_vm_mapping(&key).await.map(|m| m.vm_id);

    if let Some(vm_id) = vm_id_opt {
        match provider.restore_node_vm(&flow_id, &node_id, vm_id).await {
            Ok(vm) => {
                return Ok(Json(json!({
                    "vm_id": vm.vm_id,
                    "tier": vm.tier,
                    "guest_ip": vm.guest_ip,
                    "ssh_port": vm.ssh_port,
                    "web_port": vm.web_port,
                    "ssh_command": vm.ssh_command,
                    "web_terminal": vm.web_terminal,
                    "pid": vm.pid,
                })));
            }
            Err(crate::sandbox::error::SandboxError::NotFound(_)) => {
                // VM was deleted externally — clear the stale mapping
                repo.remove_vm_mapping(&key).await;
                tracing::warn!(
                    flow_id = %flow_id,
                    node_id = %node_id,
                    vm_id = vm_id,
                    "VM from sessions.yaml no longer exists on VM Manager"
                );
                // Fall through to NOT_FOUND
            }
            Err(e) => {
                tracing::warn!(
                    flow_id = %flow_id,
                    node_id = %node_id,
                    vm_id = vm_id,
                    error = %e,
                    "failed to restore VM from sessions.yaml"
                );
                // Fall through to NOT_FOUND
            }
        }
    }

    Err((StatusCode::NOT_FOUND, Json(json!({ "error": "No VM exists for this node" }))))
}

/// POST /api/sandbox/vm/{flow_id}/{node_id} — create (or get existing) VM for an executor node.
///
/// Before creating a new VM, checks vm_mappings (sessions.yaml) for a persisted vm_id
/// and attempts to reconnect to that VM if it's still alive.
pub(crate) async fn create_node_vm(
    State(state): State<AppState>,
    Path((flow_id, node_id)): Path<(String, String)>,
    Json(body): Json<VmCreateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let provider = require_vm_manager(&state)?;
    let repo = SandboxRepository::new(state.vm_mappings.clone());

    // Check if we have a persisted vm_id for this node from sessions.yaml
    let key = format!("{}::{}", flow_id, node_id);
    let persisted_vm_id = repo.get_vm_mapping(&key).await.map(|m| m.vm_id);

    match provider
        .get_or_create_vm_with_persisted(
            &flow_id,
            &node_id,
            body.tier.as_deref(),
            body.api_key.as_deref(),
            persisted_vm_id,
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
