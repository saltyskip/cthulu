use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;
use cthulu::cloud::vm_pool::VmStatus;

// ---------------------------------------------------------------------------
// Pool status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn cloud_pool_status(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return Ok(json!({
                "enabled": false,
                "error": "Cloud VM pool not configured (set VM_MANAGER_URL)",
            }));
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

    let vm_list: Vec<Value> = vms
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

    Ok(json!({
        "enabled": true,
        "total": total,
        "idle": idle,
        "assigned": assigned,
        "errored": errored,
        "vms": vm_list,
    }))
}

// ---------------------------------------------------------------------------
// Pool health
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn cloud_pool_health(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return Err("Cloud VM pool not configured".to_string());
        }
    };

    let results = pool.health_check().await;
    let healthy = results.iter().filter(|(_, ok)| *ok).count();
    let total = results.len();

    let details: Vec<Value> = results
        .iter()
        .map(|(id, ok)| json!({"vm_id": id, "healthy": ok}))
        .collect();

    Ok(json!({
        "healthy": healthy,
        "total": total,
        "vms": details,
    }))
}

// ---------------------------------------------------------------------------
// Test agent
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TestAgentRequest {
    message: String,
}

#[tauri::command]
pub async fn cloud_test_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: TestAgentRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pool = match &state.vm_pool {
        Some(p) => p,
        None => {
            return Err("Cloud VM pool not configured".to_string());
        }
    };

    // Acquire an idle VM
    let vm = pool
        .acquire("test")
        .await
        .ok_or_else(|| "No idle VMs available in pool".to_string())?;

    let vm_id = vm.info.vm_id;
    let a2a_url = vm.a2a_url.clone();

    // Send message via A2A
    let result = state
        .a2a_client
        .send_message(&a2a_url, &request.message, None)
        .await;

    // Release VM back to pool
    pool.release(vm_id).await;

    match result {
        Ok(task) => {
            let text = cthulu::cloud::A2aClient::extract_text(&task);
            let state_str = task
                .status
                .as_ref()
                .map(|s| format!("{:?}", s.state))
                .unwrap_or_else(|| "unknown".to_string());

            Ok(json!({
                "ok": true,
                "vm_id": vm_id,
                "a2a_url": a2a_url,
                "task_id": task.id,
                "state": state_str,
                "response": text,
            }))
        }
        Err(e) => Err(format!("A2A test message failed: {e}")),
    }
}
