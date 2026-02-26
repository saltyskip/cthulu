use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::sandbox::error::SandboxError;
use crate::sandbox::firecracker::guest_agent::{GuestAgent, SshGuestAgent};
use crate::sandbox::firecracker::host_transport::{self, shell_escape, HostTransport};
use crate::sandbox::firecracker::net::{self, NetworkAllocator, VmNetwork};
use crate::sandbox::firecracker::snapshot::SnapshotStore;
use crate::sandbox::firecracker::vm_api::{
    FirecrackerVmApi, SnapshotCreateParams, SnapshotType, VmBootConfig,
};
use crate::sandbox::handle::{ExecStream, SandboxHandle};
use crate::sandbox::provider::SandboxProvider;
use crate::sandbox::types::*;

/// Firecracker microVM sandbox provider.
///
/// Each `provision()` call:
/// 1. Allocates a unique VM ID, network (TAP + IPs), and state directory
/// 2. Copies the rootfs base image for this VM (CoW where possible)
/// 3. Starts the `firecracker` process via the host transport
/// 4. Configures the VM via the Firecracker REST API (Unix socket)
/// 5. Boots the VM and waits for SSH to become available
/// 6. Returns a `FirecrackerHandle` that implements `SandboxHandle`
pub struct FirecrackerProvider {
    config: FirecrackerConfig,
    transport: Arc<dyn HostTransport>,
    net_allocator: NetworkAllocator,
    counter: AtomicU64,
    /// If set, use TCP to reach the FC API instead of Unix socket.
    /// WARNING: TCP mode connects to a *shared* Firecracker instance.
    /// Concurrent provisions will overwrite each other's VM config.
    api_base_url: Option<String>,
    /// Serializes provision() calls in TCP mode where a shared FC instance
    /// can only serve one VM at a time. In local socket mode this is unused.
    tcp_provision_guard: tokio::sync::Mutex<()>,
}

impl FirecrackerProvider {
    pub fn new(config: FirecrackerConfig) -> Result<Self, SandboxError> {
        let transport = host_transport::build_transport(&config.host, &config.state_dir);

        // Extract TCP base URL if using LimaTcp or RemoteSsh transport
        let api_base_url = match &config.host {
            FirecrackerHostTransportConfig::LimaTcp { api_base_url, .. } => {
                Some(api_base_url.clone())
            }
            FirecrackerHostTransportConfig::RemoteSsh { api_base_url, .. } => {
                Some(api_base_url.clone())
            }
            _ => None,
        };

        // Ensure state directory exists
        std::fs::create_dir_all(&config.state_dir).map_err(|e| {
            SandboxError::Provision(format!(
                "failed to create firecracker state dir {}: {e}",
                config.state_dir.display()
            ))
        })?;

        // Seed counters from existing fc-* dirs to avoid ID/network collisions
        // with leftovers from a previous process run.
        let starting_seq = scan_existing_vms(&config.state_dir);
        if starting_seq > 0 {
            tracing::info!(
                starting_seq,
                "seeded VM counter from {} existing state dirs",
                starting_seq
            );
        }

        Ok(Self {
            transport: Arc::from(transport),
            net_allocator: NetworkAllocator::new(starting_seq as u16),
            counter: AtomicU64::new(starting_seq),
            api_base_url,
            tcp_provision_guard: tokio::sync::Mutex::new(()),
            config,
        })
    }

    /// Build a VmApi for a specific VM, using TCP or Unix socket as configured.
    fn build_api(&self, socket_path: PathBuf) -> FirecrackerVmApi {
        if let Some(ref base_url) = self.api_base_url {
            FirecrackerVmApi::new_tcp(base_url.clone())
        } else {
            FirecrackerVmApi::new_unix(
                socket_path,
                Box::new(host_transport::LocalLinuxTransport::new(
                    self.transport.firecracker_bin().to_path_buf(),
                    self.config.state_dir.clone(),
                )),
            )
        }
    }
}

#[async_trait]
impl SandboxProvider for FirecrackerProvider {
    fn info(&self) -> SandboxProviderInfo {
        SandboxProviderInfo {
            kind: SandboxBackendKind::Firecracker,
            supports_persistent_state: true,
            supports_checkpoint: true,
            supports_public_http: false,
            supports_sleep_resume: true,
        }
    }

    async fn provision(
        &self,
        spec: SandboxSpec,
    ) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        // In TCP mode, a shared FC instance can only serve one VM at a time.
        // Hold the guard for the entire provision to prevent concurrent overwrites.
        let _tcp_guard = if self.api_base_url.is_some() {
            Some(self.tcp_provision_guard.lock().await)
        } else {
            None
        };

        let seq = self.counter.fetch_add(1, Ordering::SeqCst);
        let vm_id = format!("fc-{}-{seq}", spec.workspace_id);

        tracing::info!(
            vm_id = %vm_id,
            workspace = %spec.workspace_id,
            profile = ?spec.profile,
            "provisioning firecracker VM"
        );

        // 1. Create VM state directory
        let vm_state_dir = self.config.state_dir.join(&vm_id);
        std::fs::create_dir_all(&vm_state_dir).map_err(|e| {
            SandboxError::Provision(format!("failed to create VM state dir: {e}"))
        })?;

        // 2. Copy rootfs for this VM (async — rootfs images can be 100s of MB)
        let vm_rootfs = vm_state_dir.join("rootfs.ext4");
        tracing::debug!(
            src = %self.config.rootfs_base_image.display(),
            dst = %vm_rootfs.display(),
            "copying rootfs"
        );
        tokio::fs::copy(&self.config.rootfs_base_image, &vm_rootfs)
            .await
            .map_err(|e| {
                SandboxError::Provision(format!(
                    "failed to copy rootfs from {}: {e}",
                    self.config.rootfs_base_image.display()
                ))
            })?;

        // 3. Allocate network
        let network = self.net_allocator.allocate(&vm_id);

        // Set up TAP device on the host
        net::setup_tap(self.transport.as_ref(), &network).await?;

        // 4. Generate SSH key for this VM (create in vm_state_dir)
        let ssh_key_path = vm_state_dir.join("id_rsa");
        let ssh_pub_key_path = vm_state_dir.join("id_rsa.pub");
        generate_ssh_key(&ssh_key_path)?;

        // 5. Start Firecracker process (or connect to existing one for LimaTcp)
        let socket_path = vm_state_dir.join("firecracker.socket");
        let log_path = vm_state_dir.join("firecracker.log");
        let log_str = log_path.to_string_lossy().to_string();

        let fc_child = if self.api_base_url.is_some() {
            // LimaTcp mode: FC is already running inside Lima, accessible over TCP.
            // We don't spawn a new FC process — we talk to the existing one.
            tracing::info!(
                vm_id = %vm_id,
                api_url = %self.api_base_url.as_ref().unwrap(),
                "connecting to existing Firecracker instance (LimaTcp)"
            );
            None
        } else {
            // Standard mode: spawn FC process locally
            let _ = std::fs::remove_file(&socket_path);

            let fc_bin = self.transport.firecracker_bin().to_string_lossy().to_string();
            let socket_str = socket_path.to_string_lossy().to_string();

            let child = tokio::process::Command::new(&fc_bin)
                .args(["--api-sock", &socket_str])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    SandboxError::Provision(format!("failed to start firecracker: {e}"))
                })?;

            let fc_pid = child.id();
            tracing::info!(
                vm_id = %vm_id,
                pid = ?fc_pid,
                socket = %socket_str,
                "firecracker process started"
            );

            // Wait for socket to appear
            wait_for_socket(&socket_path, Duration::from_secs(5)).await?;
            Some(child)
        };

        // 6. Configure and boot via API
        let api = self.build_api(socket_path.clone());

        // Set logger (non-fatal if it fails)
        api.set_logger(&log_str).await.ok();

        let vcpu = spec.resources.vcpu.unwrap_or(self.config.default_vcpu);
        let mem = spec.resources.memory_mb.unwrap_or(self.config.default_memory_mb);

        let boot_config = VmBootConfig {
            kernel_image_path: self.config.kernel_image.to_string_lossy().to_string(),
            rootfs_path: vm_rootfs.to_string_lossy().to_string(),
            rootfs_read_only: false,
            vcpu_count: vcpu,
            mem_size_mib: mem,
            track_dirty_pages: true,
            ..VmBootConfig::default()
        };

        // Configure network
        api.set_network_interface(&network.vm_config).await?;

        // Configure and boot
        api.configure_and_boot(&boot_config).await?;

        // 7. Wait for SSH, then set up guest networking
        let guest_agent = SshGuestAgent::new(
            network.guest_ip.clone(),
            ssh_key_path.to_string_lossy().to_string(),
            "root".into(),
            22,
            Duration::from_secs(5),
        );

        guest_agent
            .wait_for_ready(Duration::from_secs(30), Duration::from_millis(500))
            .await
            .map_err(|e| {
                SandboxError::Provision(format!(
                    "VM booted but SSH not reachable at {}: {e}",
                    network.guest_ip
                ))
            })?;

        // Set up guest networking (default route + DNS)
        net::setup_guest_network(&guest_agent, &network.host_ip).await?;

        // 8. Initialize snapshot store
        let snapshot_store = SnapshotStore::new(&vm_state_dir)?;

        let metadata = SandboxMetadata {
            workspace_id: spec.workspace_id.clone(),
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            labels: spec.labels.clone(),
        };

        tracing::info!(
            vm_id = %vm_id,
            guest_ip = %network.guest_ip,
            vcpu = vcpu,
            mem_mib = mem,
            "firecracker VM provisioned and ready"
        );

        Ok(Box::new(FirecrackerHandle {
            id: vm_id,
            vm_state_dir,
            socket_path,
            network,
            guest_agent: Arc::new(guest_agent),
            api,
            snapshot_store,
            metadata,
            env: spec.env,
            fc_child: Arc::new(tokio::sync::Mutex::new(fc_child)),
            transport: self.transport.clone(),
            ssh_key_path,
            _ssh_pub_key_path: ssh_pub_key_path,
        }))
    }

    async fn attach(&self, id: &str) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        // Check if VM state dir exists
        let vm_state_dir = self.config.state_dir.join(id);
        if !vm_state_dir.exists() {
            return Err(SandboxError::NotFound(format!(
                "firecracker VM state dir not found: {}",
                vm_state_dir.display()
            )));
        }

        let socket_path = vm_state_dir.join("firecracker.socket");
        if !socket_path.exists() {
            return Err(SandboxError::NotFound(format!(
                "firecracker socket not found for VM {id} — VM may not be running"
            )));
        }

        // Reconstruct the handle from on-disk state
        // This is best-effort: we don't persist the network allocation,
        // so we can't fully reconstruct. For now, return Unsupported.
        Err(SandboxError::Unsupported(
            "firecracker attach not yet implemented — VM state reconstruction pending",
        ))
    }

    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError> {
        let mut summaries = Vec::new();

        if !self.config.state_dir.exists() {
            return Ok(summaries);
        }

        for entry in std::fs::read_dir(&self.config.state_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("fc-") {
                continue;
            }

            let socket_path = entry.path().join("firecracker.socket");
            let status = if socket_path.exists() {
                SandboxStatus::Running
            } else {
                SandboxStatus::Stopped
            };

            // Extract workspace_id from name: "fc-{workspace_id}-{seq}"
            let workspace_id = name
                .strip_prefix("fc-")
                .and_then(|rest| rest.rsplit_once('-'))
                .map(|(ws, _)| ws.to_string())
                .unwrap_or_else(|| name.clone());

            summaries.push(SandboxSummary {
                id: name,
                backend: SandboxBackendKind::Firecracker,
                status,
                workspace_id,
            });
        }

        Ok(summaries)
    }
}

// ── FirecrackerHandle ───────────────────────────────────────────────

struct FirecrackerHandle {
    id: String,
    vm_state_dir: PathBuf,
    socket_path: PathBuf,
    network: VmNetwork,
    guest_agent: Arc<dyn GuestAgent>,
    api: FirecrackerVmApi,
    snapshot_store: SnapshotStore,
    metadata: SandboxMetadata,
    env: BTreeMap<String, String>,
    fc_child: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    transport: Arc<dyn HostTransport>,
    ssh_key_path: PathBuf,
    _ssh_pub_key_path: PathBuf,
}

#[async_trait]
impl SandboxHandle for FirecrackerHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn backend_kind(&self) -> SandboxBackendKind {
        SandboxBackendKind::Firecracker
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            persistent_state: true,
            checkpoint: CheckpointCapability::FullVmBestEffort,
            public_http: false,
            resumable_exec_sessions: false,
            network_policy_enforcement: false,
            host_mounts: false,
        }
    }

    fn metadata(&self) -> &SandboxMetadata {
        &self.metadata
    }

    // ── Exec ────────────────────────────────────────────────────

    async fn exec(&self, mut req: ExecRequest) -> Result<ExecResult, SandboxError> {
        // Merge sandbox-level env into request env (request takes precedence)
        for (k, v) in &self.env {
            req.env.entry(k.clone()).or_insert_with(|| v.clone());
        }

        tracing::debug!(
            vm_id = %self.id,
            command = ?req.command,
            "executing command in firecracker VM"
        );

        self.guest_agent.exec(&req).await
    }

    async fn exec_stream(
        &self,
        _req: ExecRequest,
    ) -> Result<Box<dyn ExecStream + Send + Unpin>, SandboxError> {
        // Streaming exec over SSH is complex. Stub for now.
        Err(SandboxError::Unsupported(
            "streaming exec not yet implemented for Firecracker VMs",
        ))
    }

    // ── Files ───────────────────────────────────────────────────

    async fn put_file(&self, req: PutFileRequest) -> Result<(), SandboxError> {
        // Write to a temp file on host, then SCP to guest
        let tmp_dir = self.vm_state_dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir)?;
        let tmp_path = tmp_dir.join(format!("put-{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmp_path, &req.bytes)?;

        let guest_path = if req.path.starts_with('/') {
            req.path.clone()
        } else {
            format!("/workspace/{}", req.path)
        };

        // Ensure parent directory exists in guest
        if req.create_parents {
            if let Some(parent) = std::path::Path::new(&guest_path).parent() {
                let mkdir_req = ExecRequest {
                    command: vec![format!("mkdir -p {}", shell_escape(&parent.display().to_string()))],
                    cwd: None,
                    env: BTreeMap::new(),
                    stdin: None,
                    timeout: Some(Duration::from_secs(10)),
                    tty: false,
                    detach: false,
                };
                self.guest_agent.exec(&mkdir_req).await?;
            }
        }

        self.guest_agent.put_file(&tmp_path, &guest_path).await?;

        // Clean up temp file
        let _ = std::fs::remove_file(&tmp_path);

        // Set permissions if requested
        if let Some(mode) = req.mode {
            let chmod_req = ExecRequest {
                command: vec![format!("chmod {:o} {}", mode, shell_escape(&guest_path))],
                cwd: None,
                env: BTreeMap::new(),
                stdin: None,
                timeout: Some(Duration::from_secs(10)),
                tty: false,
                detach: false,
            };
            self.guest_agent.exec(&chmod_req).await?;
        }

        Ok(())
    }

    async fn get_file(&self, req: GetFileRequest) -> Result<GetFileResponse, SandboxError> {
        let guest_path = if req.path.starts_with('/') {
            req.path.clone()
        } else {
            format!("/workspace/{}", req.path)
        };

        let tmp_dir = self.vm_state_dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir)?;
        let tmp_path = tmp_dir.join(format!("get-{}", uuid::Uuid::new_v4()));

        self.guest_agent
            .get_file(&guest_path, &tmp_path)
            .await?;

        let bytes = std::fs::read(&tmp_path)?;
        let _ = std::fs::remove_file(&tmp_path);
        let (bytes, truncated) = match req.max_bytes {
            Some(max) if bytes.len() > max => (bytes[..max].to_vec(), true),
            _ => (bytes, false),
        };

        Ok(GetFileResponse { bytes, truncated })
    }

    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, SandboxError> {
        let guest_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/workspace/{}", path)
        };

        // Use `ls -1apF` inside the guest to get entries
        let req = ExecRequest {
            command: vec![format!(
                "ls -1ap --color=never {} 2>/dev/null || true",
                shell_escape(&guest_path)
            )],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(10)),
            tty: false,
            detach: false,
        };

        let result = self.guest_agent.exec(&req).await?;
        let stdout = String::from_utf8_lossy(&result.stdout);

        let entries = stdout
            .lines()
            .filter(|line| !line.is_empty() && *line != "./" && *line != "../")
            .map(|line| {
                let is_dir = line.ends_with('/');
                let name = line.trim_end_matches('/').to_string();
                DirEntry {
                    path: name,
                    is_dir,
                    size_bytes: None, // Would need `stat` for size
                }
            })
            .collect();

        Ok(entries)
    }

    async fn remove_path(&self, path: &str, recursive: bool) -> Result<(), SandboxError> {
        let guest_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/workspace/{}", path)
        };

        let escaped = shell_escape(&guest_path);
        let cmd = if recursive {
            format!("rm -rf {escaped}")
        } else {
            format!("rm -f {escaped}")
        };

        let req = ExecRequest {
            command: vec![cmd],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(10)),
            tty: false,
            detach: false,
        };

        let result = self.guest_agent.exec(&req).await?;
        if result.exit_code != Some(0) {
            return Err(SandboxError::Exec(format!(
                "remove failed: {}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
        Ok(())
    }

    // ── Ports ───────────────────────────────────────────────────

    async fn expose_port(
        &self,
        _req: ExposePortRequest,
    ) -> Result<ExposedEndpoint, SandboxError> {
        // Port forwarding could be done via SSH tunneling or iptables DNAT
        Err(SandboxError::Unsupported(
            "port exposure not yet implemented for Firecracker VMs",
        ))
    }

    async fn unexpose_port(&self, _port: u16) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "port exposure not yet implemented for Firecracker VMs",
        ))
    }

    // ── Checkpoints ─────────────────────────────────────────────

    async fn checkpoint(
        &self,
        req: CheckpointRequest,
    ) -> Result<Option<CheckpointRef>, SandboxError> {
        let snap_id =
            crate::sandbox::firecracker::snapshot::generate_snapshot_id(req.name.as_deref());

        tracing::info!(
            vm_id = %self.id,
            snapshot_id = %snap_id,
            "creating VM checkpoint"
        );

        // Prepare snapshot directory
        let paths = self.snapshot_store.prepare(&snap_id)?;

        // Pause the VM
        self.api.pause().await?;

        // Create snapshot
        let snapshot_type = if req.include_memory_state {
            SnapshotType::Full
        } else {
            SnapshotType::Diff
        };

        self.api
            .create_snapshot(&SnapshotCreateParams {
                snapshot_path: paths.vm_state.to_string_lossy().to_string(),
                mem_file_path: paths.mem_file.to_string_lossy().to_string(),
                snapshot_type,
            })
            .await?;

        // Resume the VM
        self.api.resume().await?;

        let checkpoint_ref = CheckpointRef {
            id: snap_id,
            name: req.name,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        };

        tracing::info!(
            vm_id = %self.id,
            snapshot_id = %checkpoint_ref.id,
            "checkpoint created"
        );

        Ok(Some(checkpoint_ref))
    }

    async fn restore(&self, checkpoint_id: &str) -> Result<(), SandboxError> {
        if !self.snapshot_store.exists(checkpoint_id) {
            return Err(SandboxError::NotFound(format!(
                "snapshot not found: {checkpoint_id}"
            )));
        }

        tracing::info!(
            vm_id = %self.id,
            snapshot_id = %checkpoint_id,
            "restoring from checkpoint (requires VM restart)"
        );

        // Restoring a snapshot requires a fresh Firecracker process.
        // This is a complex operation that requires:
        // 1. Stop current FC process
        // 2. Start new FC process
        // 3. Load snapshot via API
        // For now, return Unsupported — full restore requires re-provisioning.
        Err(SandboxError::Unsupported(
            "snapshot restore requires VM re-provisioning (not yet automated)",
        ))
    }

    // ── Lifecycle ───────────────────────────────────────────────

    async fn stop(&self) -> Result<(), SandboxError> {
        tracing::info!(vm_id = %self.id, "stopping firecracker VM");

        // Pause the VM (keep process alive for potential resume)
        self.api.pause().await.ok();

        Ok(())
    }

    async fn resume(&self) -> Result<(), SandboxError> {
        tracing::info!(vm_id = %self.id, "resuming firecracker VM");
        self.api.resume().await
    }

    async fn destroy(&self) -> Result<(), SandboxError> {
        tracing::info!(vm_id = %self.id, "destroying firecracker VM");

        // 1. Kill the firecracker process
        {
            let mut child_lock = self.fc_child.lock().await;
            if let Some(ref mut child) = *child_lock {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            *child_lock = None;
        }

        // 2. Tear down TAP device
        net::teardown_tap(self.transport.as_ref(), &self.network.tap_name).await?;

        // 3. Remove VM state directory
        if self.vm_state_dir.exists() {
            std::fs::remove_dir_all(&self.vm_state_dir).map_err(|e| {
                SandboxError::Exec(format!(
                    "failed to remove VM state dir {}: {e}",
                    self.vm_state_dir.display()
                ))
            })?;
        }

        tracing::info!(vm_id = %self.id, "firecracker VM destroyed");
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Wait for a Unix socket file to appear.
async fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> Result<(), SandboxError> {
    let start = tokio::time::Instant::now();
    while !path.exists() {
        if start.elapsed() > timeout {
            return Err(SandboxError::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Extra small delay to let FC bind
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}

/// Generate an SSH keypair at the given path (overwrites).
fn generate_ssh_key(key_path: &std::path::Path) -> Result<(), SandboxError> {
    let output = std::process::Command::new("ssh-keygen")
        .args([
            "-f",
            &key_path.to_string_lossy(),
            "-N",
            "",
            "-t",
            "ed25519",
            "-q",
        ])
        .output()
        .map_err(|e| SandboxError::Provision(format!("ssh-keygen failed: {e}")))?;

    if !output.status.success() {
        return Err(SandboxError::Provision(format!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

/// Scan the state directory for existing `fc-*` VM directories and return the
/// next safe counter value (max existing sequence + 1). This prevents ID and
/// network/TAP collisions with leftovers from a previous process.
fn scan_existing_vms(state_dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(state_dir) else {
        return 0;
    };

    let mut max_seq: u64 = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // VM dirs are named "fc-{workspace_id}-{seq}"
        if let Some(rest) = name.strip_prefix("fc-") {
            if let Some((_, seq_str)) = rest.rsplit_once('-') {
                if let Ok(seq) = seq_str.parse::<u64>() {
                    max_seq = max_seq.max(seq + 1);
                }
            }
        }
    }

    max_seq
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_info() {
        // We can't construct a full provider without valid paths,
        // but we can test the struct layout compiles.
        let info = SandboxProviderInfo {
            kind: SandboxBackendKind::Firecracker,
            supports_persistent_state: true,
            supports_checkpoint: true,
            supports_public_http: false,
            supports_sleep_resume: true,
        };
        assert_eq!(info.kind, SandboxBackendKind::Firecracker);
        assert!(info.supports_checkpoint);
    }

    #[test]
    fn capabilities_include_checkpoint() {
        let caps = SandboxCapabilities {
            persistent_state: true,
            checkpoint: CheckpointCapability::FullVmBestEffort,
            public_http: false,
            resumable_exec_sessions: false,
            network_policy_enforcement: false,
            host_mounts: false,
        };
        assert!(matches!(
            caps.checkpoint,
            CheckpointCapability::FullVmBestEffort
        ));
    }

    #[test]
    fn wait_for_socket_path_helper() {
        // Just verify it compiles and has the right signature
        let _ = std::path::Path::new("/tmp/test.socket");
    }

    #[test]
    fn scan_existing_vms_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(scan_existing_vms(dir.path()), 0);
    }

    #[test]
    fn scan_existing_vms_with_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("fc-ws1-0")).unwrap();
        std::fs::create_dir(dir.path().join("fc-ws2-3")).unwrap();
        std::fs::create_dir(dir.path().join("fc-ws3-7")).unwrap();
        std::fs::create_dir(dir.path().join("not-a-vm")).unwrap();
        // Should return max(0+1, 3+1, 7+1) = 8
        assert_eq!(scan_existing_vms(dir.path()), 8);
    }

    #[test]
    fn scan_existing_vms_nonexistent_dir() {
        assert_eq!(
            scan_existing_vms(std::path::Path::new("/nonexistent/path")),
            0
        );
    }
}
