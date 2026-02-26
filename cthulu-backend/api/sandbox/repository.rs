use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use hyper::StatusCode;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::api::{AppState, VmMapping};
use crate::sandbox::backends::vm_manager::VmManagerProvider;

/// Helper to get the VmManagerProvider from AppState.
/// Returns 503 Service Unavailable with a descriptive JSON body if VM Manager is not configured.
pub fn require_vm_manager(
    state: &AppState,
) -> Result<&VmManagerProvider, (StatusCode, Json<Value>)> {
    state.vm_manager.as_deref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "VM Manager not configured. Set VM_MANAGER_URL in .env" })),
    ))
}

pub struct SandboxRepository {
    vm_mappings: Arc<RwLock<HashMap<String, VmMapping>>>,
}

impl SandboxRepository {
    pub fn new(vm_mappings: Arc<RwLock<HashMap<String, VmMapping>>>) -> Self {
        Self { vm_mappings }
    }

    pub async fn get_vm_mapping(&self, key: &str) -> Option<VmMapping> {
        self.vm_mappings.read().await.get(key).cloned()
    }

    pub async fn remove_vm_mapping(&self, key: &str) {
        self.vm_mappings.write().await.remove(key);
    }
}
