//! VM Manager sandbox backend.
//!
//! This backend talks to an external VM Manager API that handles all
//! Firecracker lifecycle: process management, rootfs, networking, web terminal.
//!
//! Cthulu acts as a relay — it creates/destroys VMs via HTTP and returns
//! the web terminal URL for the user to connect in-browser (ttyd iframe).
//!
//! VMs are persistent per executor node: one VM per (flow_id, node_id) pair.
//! The node key → vm mapping is stored in memory on the provider.
//! Users interact with VMs exclusively through the ttyd web terminal.

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

/// Node-scoped VM tracking: maps "flow_id::node_id" → VmResponse.
type NodeVmMap = Arc<RwLock<BTreeMap<String, VmResponse>>>;

/// Build the key for the node VM map: "flow_id::node_id".
fn node_vm_key(flow_id: &str, node_id: &str) -> String {
    format!("{flow_id}::{node_id}")
}

pub struct VmManagerProvider {
    client: VmManagerClient,
    config: VmManagerConfig,
    /// Persistent map of "flow_id::node_id" → VM. One VM per executor node.
    node_vms: NodeVmMap,
}

impl VmManagerProvider {
    pub fn new(config: VmManagerConfig) -> Result<Self, SandboxError> {
        let client = VmManagerClient::new(config.api_base_url.clone());
        Ok(Self {
            client,
            config,
            node_vms: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    /// Get or create a VM for a given executor node.
    ///
    /// Each executor node gets its own VM, keyed by `flow_id::node_id`.
    pub async fn get_or_create_vm(
        &self,
        flow_id: &str,
        node_id: &str,
        tier: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<VmResponse, SandboxError> {
        let key = node_vm_key(flow_id, node_id);

        // Check if VM already exists for this node
        {
            let map = self.node_vms.read().await;
            if let Some(vm) = map.get(&key) {
                // Verify it's still alive
                match self.client.get_vm(vm.vm_id).await {
                    Ok(live_vm) => {
                        tracing::debug!(
                            flow_id = %flow_id,
                            node_id = %node_id,
                            vm_id = live_vm.vm_id,
                            "reusing existing VM for node"
                        );
                        return Ok(live_vm);
                    }
                    Err(SandboxError::NotFound(_)) => {
                        tracing::warn!(
                            flow_id = %flow_id,
                            node_id = %node_id,
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
            node_id = %node_id,
            vm_id = vm.vm_id,
            tier = %vm.tier,
            web_terminal = %vm.web_terminal,
            "created new VM for node"
        );

        // Store the mapping
        {
            let mut map = self.node_vms.write().await;
            map.insert(key, vm.clone());
        }

        Ok(vm)
    }

    /// Get the VM for an executor node (if one exists).
    pub async fn get_node_vm(&self, flow_id: &str, node_id: &str) -> Option<VmResponse> {
        let key = node_vm_key(flow_id, node_id);
        let map = self.node_vms.read().await;
        map.get(&key).cloned()
    }

    /// Destroy the VM for an executor node.
    pub async fn destroy_node_vm(
        &self,
        flow_id: &str,
        node_id: &str,
    ) -> Result<Option<u32>, SandboxError> {
        let key = node_vm_key(flow_id, node_id);
        let vm = {
            let mut map = self.node_vms.write().await;
            map.remove(&key)
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

    /// Create VMs for all executor nodes in a flow.
    /// Returns `Vec<(node_id, vm_name, VmResponse)>` for each provisioned VM.
    pub async fn provision_flow_vms(
        &self,
        flow: &crate::flows::Flow,
        oauth_token: Option<&str>,
    ) -> Result<Vec<(String, String, crate::sandbox::vm_manager::VmResponse)>, SandboxError> {
        use crate::flows::NodeType;

        let executor_nodes: Vec<&crate::flows::Node> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Executor)
            .collect();

        let mut results = Vec::new();
        for node in executor_nodes {
            let tier = node.config["tier"].as_str();
            let vm = self.get_or_create_vm(&flow.id, &node.id, tier, None).await?;

            // Generate VM name: "{label}_{short_uuid}"
            let short_id = &uuid::Uuid::new_v4().to_string()[..6];
            let vm_name = format!(
                "{}_{}",
                node.label.replace(' ', "-").replace("::", "-"),
                short_id,
            );

            tracing::info!(
                flow = %flow.name,
                node = %node.label,
                vm_id = vm.vm_id,
                vm_name = %vm_name,
                "provisioned VM for executor node"
            );

            // Inject OAuth token if available
            if let Some(token) = oauth_token {
                if let Err(e) = inject_oauth_token(&vm.web_terminal, token).await {
                    tracing::warn!(
                        vm_id = vm.vm_id,
                        vm_name = %vm_name,
                        error = %e,
                        "OAuth token injection failed (user can log in manually via terminal)"
                    );
                }
            }

            results.push((node.id.clone(), vm_name, vm));
        }
        Ok(results)
    }

    /// Destroy all VMs for a flow's executor nodes.
    pub async fn destroy_flow_vms(
        &self,
        flow: &crate::flows::Flow,
    ) -> Result<(), SandboxError> {
        use crate::flows::NodeType;

        for node in flow.nodes.iter().filter(|n| n.node_type == NodeType::Executor) {
            match self.destroy_node_vm(&flow.id, &node.id).await {
                Ok(Some(vm_id)) => {
                    tracing::info!(
                        flow = %flow.name,
                        node = %node.label,
                        vm_id = vm_id,
                        "destroyed VM for executor node"
                    );
                }
                Ok(None) => {} // No VM existed for this node
                Err(e) => {
                    tracing::warn!(
                        flow = %flow.name,
                        node = %node.label,
                        error = %e,
                        "failed to destroy VM (may already be gone)"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Inject a Claude OAuth token into a VM via its ttyd web terminal WebSocket.
///
/// Uses `TtydSession` to connect, then:
/// 1. Sets `CLAUDE_CODE_OAUTH_TOKEN` env var in `~/.bashrc`
/// 2. Writes `~/.claude/.credentials.json` for Claude CLI
///
/// Uses base64 encoding to avoid shell quoting issues with special characters.
async fn inject_oauth_token(web_terminal_url: &str, token: &str) -> Result<(), SandboxError> {
    use crate::sandbox::ttyd::TtydSession;

    let mut session = TtydSession::connect(web_terminal_url).await?;
    let timeout = std::time::Duration::from_secs(10);

    // Step 1: Write CLAUDE_CODE_OAUTH_TOKEN to .bashrc via base64 to avoid quoting issues
    let bashrc_line = format!("export CLAUDE_CODE_OAUTH_TOKEN='{}'", token);
    let bashrc_b64 = base64_encode_simple(bashrc_line.as_bytes());
    let cmd1 = format!(
        "grep -q 'CLAUDE_CODE_OAUTH_TOKEN' ~/.bashrc 2>/dev/null || echo '{}' | base64 -d >> ~/.bashrc",
        bashrc_b64
    );
    if let Err(e) = session.exec(&cmd1, timeout).await {
        tracing::warn!(error = %e, "Failed to write token to .bashrc");
    }

    // Step 2: Create .claude dir and write credentials.json via base64
    let credentials_json = serde_json::json!({
        "claudeAiOauth": {
            "accessToken": token,
            "scopes": ["user:inference"],
        }
    });
    let creds_str = serde_json::to_string(&credentials_json).unwrap_or_default();
    let creds_b64 = base64_encode_simple(creds_str.as_bytes());
    let cmd2 = format!(
        "mkdir -p ~/.claude && echo '{}' | base64 -d > ~/.claude/.credentials.json && chmod 600 ~/.claude/.credentials.json",
        creds_b64
    );
    if let Err(e) = session.exec(&cmd2, timeout).await {
        tracing::warn!(error = %e, "Failed to write credentials.json");
    }

    // Step 3: Source .bashrc so the env var is active for subsequent commands
    let _ = session.exec("source ~/.bashrc", timeout).await;

    tracing::info!("OAuth token injected successfully via ttyd");
    session.close().await;
    Ok(())
}

/// Simple base64 encoding (same algorithm as vm_executor.rs).
fn base64_encode_simple(data: &[u8]) -> String {
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
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
        // workspace_id is used as both flow_id and node_id fallback
        let vm = self
            .get_or_create_vm(&spec.workspace_id, "default", None, None)
            .await?;

        let key = node_vm_key(&spec.workspace_id, "default");
        Ok(Box::new(VmManagerHandle {
            vm_id: vm.vm_id,
            node_key: key,
            vm_info: vm,
            client: self.client.clone(),
            node_vms: self.node_vms.clone(),
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
            node_key: String::new(), // Unknown node when attaching by raw id
            vm_info: vm,
            client: self.client.clone(),
            node_vms: self.node_vms.clone(),
            metadata: SandboxMetadata {
                workspace_id: id.to_string(),
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                labels: BTreeMap::new(),
            },
        }))
    }

    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError> {
        let list = self.client.list_vms().await?;
        let node_map = self.node_vms.read().await;

        // Build reverse map: vm_id → node_key
        let vm_to_node: BTreeMap<u32, String> = node_map
            .iter()
            .map(|(node_key, vm)| (vm.vm_id, node_key.clone()))
            .collect();

        Ok(list
            .vms
            .values()
            .map(|vm| SandboxSummary {
                id: vm.vm_id.to_string(),
                backend: SandboxBackendKind::VmManager,
                status: SandboxStatus::Running,
                workspace_id: vm_to_node
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
/// via ttyd web terminal in browser). The handle primarily exists for
/// lifecycle management (destroy) and metadata.
struct VmManagerHandle {
    vm_id: u32,
    node_key: String,
    vm_info: VmResponse,
    client: VmManagerClient,
    node_vms: NodeVmMap,
    metadata: SandboxMetadata,
}

#[async_trait]
impl SandboxHandle for VmManagerHandle {
    fn id(&self) -> &str {
        &self.node_key
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

    // ── Exec (not used — VMs are interactive via ttyd terminal) ─

    async fn exec(&self, _req: ExecRequest) -> Result<ExecResult, SandboxError> {
        // VMs are interactive-only. Users connect via ttyd web terminal (iframe).
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
            node_key = %self.node_key,
            "destroying VM Manager VM"
        );

        // Remove from node map
        {
            let mut map = self.node_vms.write().await;
            map.remove(&self.node_key);
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

    #[test]
    fn node_vm_key_format() {
        assert_eq!(node_vm_key("flow-1", "node-a"), "flow-1::node-a");
    }

    #[tokio::test]
    async fn node_vm_map_operations() {
        let map: NodeVmMap = Arc::new(RwLock::new(BTreeMap::new()));

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
        let key = node_vm_key("flow-1", "node-a");
        map.write().await.insert(key.clone(), vm);

        // Retrieve
        let read = map.read().await;
        assert!(read.contains_key(&key));
        assert_eq!(read.get(&key).unwrap().vm_id, 0);
        drop(read);

        // Remove
        let removed = map.write().await.remove(&key);
        assert!(removed.is_some());
        assert!(map.read().await.is_empty());
    }
}
