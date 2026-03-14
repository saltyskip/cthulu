//! Cloud VM pool REST endpoints.
//!
//! GET  /api/cloud/pool          — pool status (VM list, idle/busy counts)
//! GET  /api/cloud/pool/health   — health check all VMs (ping A2A endpoints)
//! POST /api/cloud/pool/test     — send a test message to an idle VM agent

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::AppState;
use crate::cloud::vm_pool::VmStatus;

/// GET /api/cloud/pool — return the current pool state.
pub(crate) async fn pool_status(State(state): State<AppState>) -> impl IntoResponse {
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return Json(json!({
                "enabled": false,
                "error": "Cloud VM pool not configured (set VM_MANAGER_URL)"
            }))
            .into_response()
        }
    };

    let vms = pool.status().await;
    let total = vms.len();
    let idle = vms.iter().filter(|v| v.status == VmStatus::Idle).count();
    let assigned = vms
        .iter()
        .filter(|v| matches!(v.status, VmStatus::Assigned { .. }))
        .count();
    let errored = vms
        .iter()
        .filter(|v| matches!(v.status, VmStatus::Error(_)))
        .count();

    let vm_list: Vec<_> = vms
        .iter()
        .map(|v| {
            let status_str = match &v.status {
                VmStatus::Idle => "idle".to_string(),
                VmStatus::Assigned { workflow_run_id } => {
                    format!("assigned:{workflow_run_id}")
                }
                VmStatus::Provisioning => "provisioning".to_string(),
                VmStatus::Error(e) => format!("error:{e}"),
            };
            json!({
                "vm_id": v.info.vm_id,
                "ssh_port": v.info.ssh_port,
                "web_port": v.info.web_port,
                "a2a_url": v.a2a_url,
                "tier": v.info.tier,
                "status": status_str,
            })
        })
        .collect();

    Json(json!({
        "enabled": true,
        "total": total,
        "idle": idle,
        "assigned": assigned,
        "errored": errored,
        "vms": vm_list,
    }))
    .into_response()
}

/// GET /api/cloud/pool/health — ping each VM's A2A agent card endpoint.
pub(crate) async fn pool_health(State(state): State<AppState>) -> impl IntoResponse {
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "Cloud VM pool not configured"})),
            )
                .into_response()
        }
    };

    let results = pool.health_check().await;
    let healthy = results.iter().filter(|(_, ok)| *ok).count();
    let total = results.len();

    let details: Vec<_> = results
        .iter()
        .map(|(id, ok)| json!({"vm_id": id, "healthy": ok}))
        .collect();

    Json(json!({
        "healthy": healthy,
        "total": total,
        "vms": details,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub(crate) struct TestAgentRequest {
    /// Message to send to the agent.
    message: String,
}

/// POST /api/cloud/pool/test — send a test message to an idle VM's A2A agent.
pub(crate) async fn test_agent(
    State(state): State<AppState>,
    Json(body): Json<TestAgentRequest>,
) -> impl IntoResponse {
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "Cloud VM pool not configured"})),
            )
                .into_response()
        }
    };

    // Acquire an idle VM
    let vm = match pool.acquire("test").await {
        Some(v) => v,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "No idle VMs available in pool"})),
            )
                .into_response()
        }
    };

    let vm_id = vm.info.vm_id;
    let a2a_url = vm.a2a_url.clone();

    // Send message via A2A
    let result = state
        .a2a_client
        .send_message(&a2a_url, &body.message, None)
        .await;

    // Release VM back to pool
    pool.release(vm_id).await;

    match result {
        Ok(task) => {
            let text = crate::cloud::A2aClient::extract_text(&task);
            let state_str = task
                .status
                .as_ref()
                .map(|s| format!("{:?}", s.state))
                .unwrap_or_else(|| "unknown".to_string());

            Json(json!({
                "ok": true,
                "vm_id": vm_id,
                "a2a_url": a2a_url,
                "task_id": task.id,
                "state": state_str,
                "response": text,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!(vm_id = vm_id, error = %e, "A2A test message failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "ok": false,
                    "vm_id": vm_id,
                    "error": e.to_string(),
                })),
            )
                .into_response()
        }
    }
}
