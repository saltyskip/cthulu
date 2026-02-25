//! VM Manager sandbox backend.
//!
//! This backend talks to an external VM Manager API that handles all
//! Firecracker lifecycle: process management, rootfs, networking, web terminal.
//!
//! Cthulu acts as a relay — it creates/destroys VMs via HTTP and returns
//! the web terminal URL for the user to connect in-browser.
//!
//! VMs are persistent per flow: one VM per flow, reused across interactions.
//! The flow_id → vm mapping is stored in memory on the provider.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::sandbox::error::SandboxError;
use crate::sandbox::handle::{ExecStream, SandboxHandle};
use crate::sandbox::provider::SandboxProvider;
use crate::sandbox::types::*;
use crate::sandbox::vm_manager::{VmCreateRequest, VmManagerClient, VmResponse};

// ── Provider ────────────────────────────────────────────────────────

/// Flow-scoped VM tracking: maps flow_id → VmResponse.
type FlowVmMap = Arc<RwLock<BTreeMap<String, VmResponse>>>;

pub struct VmManagerProvider {
    client: VmManagerClient,
    config: VmManagerConfig,
    /// Persistent map of flow_id → VM. Survives across multiple provision() calls.
    flow_vms: FlowVmMap,
}

impl VmManagerProvider {
    pub fn new(config: VmManagerConfig) -> Result<Self, SandboxError> {
        let client = VmManagerClient::new(config.api_base_url.clone());
        Ok(Self {
            client,
            config,
            flow_vms: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    /// Get or create a VM for a given flow.
    pub async fn get_or_create_vm(
        &self,
        flow_id: &str,
        tier: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<VmResponse, SandboxError> {
        // Check if VM already exists for this flow
        {
            let map = self.flow_vms.read().await;
            if let Some(vm) = map.get(flow_id) {
                // Verify it's still alive
                match self.client.get_vm(vm.vm_id).await {
                    Ok(live_vm) => {
                        tracing::debug!(
                            flow_id = %flow_id,
                            vm_id = live_vm.vm_id,
                            "reusing existing VM for flow"
                        );
                        return Ok(live_vm);
                    }
                    Err(SandboxError::NotFound(_)) => {
                        tracing::warn!(
                            flow_id = %flow_id,
                            vm_id = vm.vm_id,
                            "VM was deleted externally, will create new one"
                        );
                        // Fall through to create a new one
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Create a new VM
        let tier = tier.unwrap_or(&self.config.default_tier);
        let api_key = api_key
            .map(|s| s.to_string())
            .or_else(|| self.config.api_key.clone())
            .unwrap_or_default();

        let req = VmCreateRequest {
            tier: tier.to_string(),
            api_key,
        };

        let vm = self.client.create_vm(&req).await?;

        tracing::info!(
            flow_id = %flow_id,
            vm_id = vm.vm_id,
            tier = %vm.tier,
            web_terminal = %vm.web_terminal,
            "created new VM for flow"
        );

        // Store the mapping
        {
            let mut map = self.flow_vms.write().await;
            map.insert(flow_id.to_string(), vm.clone());
        }

        Ok(vm)
    }

    /// Get the VM for a flow (if one exists).
    pub async fn get_flow_vm(&self, flow_id: &str) -> Option<VmResponse> {
        let map = self.flow_vms.read().await;
        map.get(flow_id).cloned()
    }

    /// Destroy the VM for a flow.
    pub async fn destroy_flow_vm(&self, flow_id: &str) -> Result<Option<u32>, SandboxError> {
        let vm = {
            let mut map = self.flow_vms.write().await;
            map.remove(flow_id)
        };

        if let Some(vm) = vm {
            let vm_id = vm.vm_id;
            self.client.delete_vm(vm_id).await?;
            Ok(Some(vm_id))
        } else {
            Ok(None)
        }
    }

    /// Get the underlying client (for health checks from routes).
    pub fn client(&self) -> &VmManagerClient {
        &self.client
    }
}

#[async_trait]
impl SandboxProvider for VmManagerProvider {
    fn info(&self) -> SandboxProviderInfo {
        SandboxProviderInfo {
            kind: SandboxBackendKind::VmManager,
            supports_persistent_state: true,
            supports_checkpoint: false,
            supports_public_http: true,
            supports_sleep_resume: false,
        }
    }

    async fn provision(
        &self,
        spec: SandboxSpec,
    ) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        let vm = self
            .get_or_create_vm(&spec.workspace_id, None, None)
            .await?;

        Ok(Box::new(VmManagerHandle {
            vm_id: vm.vm_id,
            flow_id: spec.workspace_id.clone(),
            vm_info: vm,
            client: self.client.clone(),
            flow_vms: self.flow_vms.clone(),
            metadata: SandboxMetadata {
                workspace_id: spec.workspace_id,
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                labels: spec.labels,
            },
        }))
    }

    async fn attach(&self, id: &str) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        // Parse vm_id from the string id
        let vm_id: u32 = id
            .parse()
            .map_err(|_| SandboxError::NotFound(format!("invalid VM id: {id}")))?;

        let vm = self.client.get_vm(vm_id).await?;

        Ok(Box::new(VmManagerHandle {
            vm_id: vm.vm_id,
            flow_id: String::new(), // Unknown flow when attaching by raw id
            vm_info: vm,
            client: self.client.clone(),
            flow_vms: self.flow_vms.clone(),
            metadata: SandboxMetadata {
                workspace_id: id.to_string(),
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                labels: BTreeMap::new(),
            },
        }))
    }

    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError> {
        let list = self.client.list_vms().await?;
        let flow_map = self.flow_vms.read().await;

        // Build reverse map: vm_id → flow_id
        let vm_to_flow: BTreeMap<u32, String> = flow_map
            .iter()
            .map(|(flow_id, vm)| (vm.vm_id, flow_id.clone()))
            .collect();

        Ok(list
            .vms
            .values()
            .map(|vm| SandboxSummary {
                id: vm.vm_id.to_string(),
                backend: SandboxBackendKind::VmManager,
                status: SandboxStatus::Running,
                workspace_id: vm_to_flow
                    .get(&vm.vm_id)
                    .cloned()
                    .unwrap_or_else(|| format!("vm-{}", vm.vm_id)),
            })
            .collect())
    }
}

// ── Handle ──────────────────────────────────────────────────────────

/// Handle to a VM managed by the VM Manager.
///
/// This is a thin wrapper — the VM is interactive-only (user connects
/// via web terminal in browser). The handle primarily exists for
/// lifecycle management (destroy) and metadata.
struct VmManagerHandle {
    vm_id: u32,
    flow_id: String,
    vm_info: VmResponse,
    client: VmManagerClient,
    flow_vms: FlowVmMap,
    metadata: SandboxMetadata,
}

#[async_trait]
impl SandboxHandle for VmManagerHandle {
    fn id(&self) -> &str {
        // Return a static-lifetime-safe ID by leaking — or use a stored String.
        // Since id() returns &str, we need a field. Use flow_id as the ID.
        &self.flow_id
    }

    fn backend_kind(&self) -> SandboxBackendKind {
        SandboxBackendKind::VmManager
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            persistent_state: true,
            checkpoint: CheckpointCapability::None,
            public_http: true,
            resumable_exec_sessions: false,
            network_policy_enforcement: false,
            host_mounts: false,
        }
    }

    fn metadata(&self) -> &SandboxMetadata {
        &self.metadata
    }

    // ── Exec (not used — VMs are interactive only) ──────────────

    async fn exec(&self, _req: ExecRequest) -> Result<ExecResult, SandboxError> {
        // VMs are interactive-only. Users connect via web terminal.
        // If programmatic exec is ever needed, it would SSH into the VM.
        Err(SandboxError::Unsupported(
            "VM Manager VMs are interactive-only — use the web terminal",
        ))
    }

    async fn exec_stream(
        &self,
        _req: ExecRequest,
    ) -> Result<Box<dyn ExecStream + Send + Unpin>, SandboxError> {
        Err(SandboxError::Unsupported(
            "VM Manager VMs are interactive-only — use the web terminal",
        ))
    }

    // ── Files (not used) ────────────────────────────────────────

    async fn put_file(&self, _req: PutFileRequest) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "file operations not supported for VM Manager VMs — use the web terminal",
        ))
    }

    async fn get_file(&self, _req: GetFileRequest) -> Result<GetFileResponse, SandboxError> {
        Err(SandboxError::Unsupported(
            "file operations not supported for VM Manager VMs — use the web terminal",
        ))
    }

    async fn read_dir(&self, _path: &str) -> Result<Vec<DirEntry>, SandboxError> {
        Err(SandboxError::Unsupported(
            "file operations not supported for VM Manager VMs — use the web terminal",
        ))
    }

    async fn remove_path(&self, _path: &str, _recursive: bool) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "file operations not supported for VM Manager VMs — use the web terminal",
        ))
    }

    // ── Ports ───────────────────────────────────────────────────

    async fn expose_port(
        &self,
        _req: ExposePortRequest,
    ) -> Result<ExposedEndpoint, SandboxError> {
        Err(SandboxError::Unsupported(
            "port exposure not supported for VM Manager VMs",
        ))
    }

    async fn unexpose_port(&self, _port: u16) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "port exposure not supported for VM Manager VMs",
        ))
    }

    // ── Checkpoints ─────────────────────────────────────────────

    async fn checkpoint(
        &self,
        _req: CheckpointRequest,
    ) -> Result<Option<CheckpointRef>, SandboxError> {
        Err(SandboxError::Unsupported(
            "checkpoints not supported for VM Manager VMs",
        ))
    }

    async fn restore(&self, _checkpoint_id: &str) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "checkpoints not supported for VM Manager VMs",
        ))
    }

    // ── Lifecycle ───────────────────────────────────────────────

    async fn stop(&self) -> Result<(), SandboxError> {
        // No-op — VM persists for the life of the flow.
        tracing::debug!(vm_id = self.vm_id, "stop called (no-op for VmManager)");
        Ok(())
    }

    async fn resume(&self) -> Result<(), SandboxError> {
        // No-op — VM is always running.
        Ok(())
    }

    async fn destroy(&self) -> Result<(), SandboxError> {
        tracing::info!(
            vm_id = self.vm_id,
            flow_id = %self.flow_id,
            "destroying VM Manager VM"
        );

        // Remove from flow map
        {
            let mut map = self.flow_vms.write().await;
            map.remove(&self.flow_id);
        }

        // Delete via API
        self.client.delete_vm(self.vm_id).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_info() {
        let config = VmManagerConfig {
            api_base_url: "http://localhost:8080".into(),
            ssh_host: "localhost".into(),
            default_tier: "nano".into(),
            api_key: None,
        };
        let provider = VmManagerProvider::new(config).unwrap();
        let info = provider.info();
        assert_eq!(info.kind, SandboxBackendKind::VmManager);
        assert!(!info.supports_checkpoint);
        assert!(info.supports_public_http);
    }

    #[test]
    fn handle_capabilities() {
        let caps = SandboxCapabilities {
            persistent_state: true,
            checkpoint: CheckpointCapability::None,
            public_http: true,
            resumable_exec_sessions: false,
            network_policy_enforcement: false,
            host_mounts: false,
        };
        assert!(matches!(caps.checkpoint, CheckpointCapability::None));
        assert!(caps.public_http);
    }

    #[tokio::test]
    async fn flow_vm_map_operations() {
        let map: FlowVmMap = Arc::new(RwLock::new(BTreeMap::new()));

        // Initially empty
        assert!(map.read().await.is_empty());

        // Insert
        let vm = VmResponse {
            vm_id: 0,
            tier: "nano".into(),
            guest_ip: "172.16.0.2".into(),
            ssh_port: 2222,
            web_port: 7700,
            ssh_command: "ssh -p 2222 root@localhost".into(),
            web_terminal: "http://localhost:7700".into(),
            pid: 100,
        };
        map.write().await.insert("flow-1".into(), vm);

        // Retrieve
        let read = map.read().await;
        assert!(read.contains_key("flow-1"));
        assert_eq!(read.get("flow-1").unwrap().vm_id, 0);
        drop(read);

        // Remove
        let removed = map.write().await.remove("flow-1");
        assert!(removed.is_some());
        assert!(map.read().await.is_empty());
    }
}
