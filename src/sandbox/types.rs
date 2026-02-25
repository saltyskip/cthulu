use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

// ── Sandbox identity ────────────────────────────────────────────────

pub type SandboxId = String;
pub type CheckpointId = String;

// ── Backend kind ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    DangerousHost,
    Firecracker,
    VmManager,
    FlySprite,
}

// ── Provider info ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxProviderInfo {
    pub kind: SandboxBackendKind,
    pub supports_persistent_state: bool,
    pub supports_checkpoint: bool,
    pub supports_public_http: bool,
    pub supports_sleep_resume: bool,
}

// ── Sandbox spec (input to provision) ───────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub workspace_id: String,
    pub profile: SandboxProfile,
    pub filesystem: FilesystemSpec,
    pub resources: ResourceHints,
    pub env: BTreeMap<String, String>,
    pub mounts: Vec<MountSpec>,
    pub network: NetworkPolicy,
    pub lifecycle: LifecyclePolicy,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum SandboxProfile {
    Base,
    PythonAgent,
    NodeAgent,
    RustAgent,
    FullDev,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct FilesystemSpec {
    pub workspace_path: String,
    pub artifacts_path: String,
    pub tmp_path: String,
    pub persistent_workspace: bool,
    pub size_mb_hint: Option<u32>,
}

impl Default for FilesystemSpec {
    fn default() -> Self {
        Self {
            workspace_path: "/workspace".into(),
            artifacts_path: "/artifacts".into(),
            tmp_path: "/tmp".into(),
            persistent_workspace: false,
            size_mb_hint: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourceHints {
    pub vcpu: Option<u8>,
    pub memory_mb: Option<u32>,
    pub disk_mb: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct MountSpec {
    pub host_path: PathBuf,
    pub guest_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone)]
pub struct LifecyclePolicy {
    pub idle_timeout: Option<Duration>,
    pub auto_sleep: bool,
    pub ttl: Option<Duration>,
}

impl Default for LifecyclePolicy {
    fn default() -> Self {
        Self {
            idle_timeout: None,
            auto_sleep: false,
            ttl: None,
        }
    }
}

// ── Network policy ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    pub mode: NetworkMode,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
    pub allow_loopback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    Disabled,
    AllowAll,
    Allowlist,
}

impl NetworkPolicy {
    pub fn default_safe() -> Self {
        Self {
            mode: NetworkMode::Disabled,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
            allow_loopback: true,
        }
    }
}

// ── Capabilities (reported by handle) ───────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxCapabilities {
    pub persistent_state: bool,
    pub checkpoint: CheckpointCapability,
    pub public_http: bool,
    pub resumable_exec_sessions: bool,
    pub network_policy_enforcement: bool,
    pub host_mounts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointCapability {
    None,
    FilesystemOnly,
    FullVmBestEffort,
}

// ── Metadata / status ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxMetadata {
    pub workspace_id: String,
    pub created_at_unix_ms: i64,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SandboxSummary {
    pub id: SandboxId,
    pub backend: SandboxBackendKind,
    pub status: SandboxStatus,
    pub workspace_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    Provisioning,
    Running,
    Sleeping,
    Stopped,
    Destroyed,
    Unknown,
}

// ── Exec model ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
    pub tty: bool,
    pub detach: bool,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub started_at_unix_ms: i64,
    pub finished_at_unix_ms: Option<i64>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ExecEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit { code: i32 },
    Status(String),
}

// ── File operations ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PutFileRequest {
    pub path: String,
    pub bytes: Vec<u8>,
    pub create_parents: bool,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct GetFileRequest {
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GetFileResponse {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
}

// ── Port exposure (stub — used by Firecracker / Sprite) ─────────────

#[derive(Debug, Clone)]
pub struct ExposePortRequest {
    pub port: u16,
    pub protocol: PortProtocol,
    pub visibility: PortVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortProtocol {
    Tcp,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortVisibility {
    LocalhostOnly,
    Public,
}

#[derive(Debug, Clone)]
pub struct ExposedEndpoint {
    pub port: u16,
    pub url: Option<String>,
    pub local_bind: Option<String>,
}

// ── Checkpoints (stub — used by Firecracker) ────────────────────────

#[derive(Debug, Clone)]
pub struct CheckpointRequest {
    pub name: Option<String>,
    pub include_memory_state: bool,
}

#[derive(Debug, Clone)]
pub struct CheckpointRef {
    pub id: CheckpointId,
    pub name: Option<String>,
    pub created_at_unix_ms: i64,
}

// ── Backend configs ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DangerousConfig {
    pub root_dir: PathBuf,
    pub inherit_env_allowlist: Vec<String>,
    pub default_shell: Vec<String>,
    pub use_dedicated_user: bool,
    pub read_only_host_mounts: Vec<PathBuf>,
    pub enable_rlimits: bool,
    pub max_output_bytes: usize,
}

impl Default for DangerousConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from(""), // must be set by caller
            inherit_env_allowlist: vec!["PATH".into(), "HOME".into(), "LANG".into(), "TERM".into()],
            default_shell: vec!["/bin/bash".into(), "-lc".into()],
            use_dedicated_user: false,
            read_only_host_mounts: vec![],
            enable_rlimits: false,
            max_output_bytes: 10 * 1024 * 1024, // 10 MB
        }
    }
}

#[derive(Debug, Clone)]
pub struct FirecrackerConfig {
    pub host: FirecrackerHostTransportConfig,
    pub state_dir: PathBuf,
    pub kernel_image: PathBuf,
    pub rootfs_base_image: PathBuf,
    pub default_vcpu: u8,
    pub default_memory_mb: u32,
    pub network: FirecrackerNetworkConfig,
    pub use_jailer: bool,
    pub guest_agent: GuestAgentTransport,
}

#[derive(Debug, Clone)]
pub enum FirecrackerHostTransportConfig {
    LocalLinux {
        firecracker_bin: PathBuf,
        jailer_bin: Option<PathBuf>,
    },
    LimaSsh {
        ssh_target: String,
        remote_firecracker_bin: String,
        remote_state_dir: String,
    },
    /// Lima VM with FC API exposed via socat/port-forward to a TCP endpoint
    /// on the macOS host. This is the easiest macOS setup.
    LimaTcp {
        /// Lima instance name (for `limactl shell`)
        lima_instance: String,
        /// TCP base URL where FC API is reachable from macOS (e.g., "http://localhost:8080")
        api_base_url: String,
        /// SSH target IP of the guest VM *inside* Lima (for running commands in the microVM)
        guest_ssh_via_lima: bool,
    },
    /// Remote Linux server with real /dev/kvm, accessed over SSH.
    /// FC API is exposed via socat TCP on the remote host.
    RemoteSsh {
        /// SSH destination (e.g., "user@192.168.1.100" or "user@myserver.com")
        ssh_target: String,
        /// SSH port (default 22)
        ssh_port: u16,
        /// Path to SSH private key (None = use ssh-agent / default key)
        ssh_key_path: Option<String>,
        /// TCP base URL where FC API is reachable from this machine
        /// (e.g., "http://myserver:8080" after socat port-forward)
        api_base_url: String,
        /// Path to firecracker binary on the remote server
        remote_firecracker_bin: String,
        /// State directory on the remote server
        remote_state_dir: String,
    },
}

#[derive(Debug, Clone)]
pub struct FirecrackerNetworkConfig {
    pub enable_internet: bool,
    pub allowed_egress: Vec<String>,
    pub host_port_range_start: u16,
    pub host_port_range_end: u16,
}

#[derive(Debug, Clone, Copy)]
pub enum GuestAgentTransport {
    Vsock,
    Ssh,
}

#[derive(Debug, Clone)]
pub struct FlySpriteConfig {
    pub api_base_url: String,
    pub api_token: String,
    pub org: Option<String>,
    pub project: Option<String>,
    pub region: Option<String>,
    pub default_profile: Option<String>,
    pub idle_timeout: Option<Duration>,
    pub file_sync_mode: SpriteFileSyncMode,
}

#[derive(Debug, Clone, Copy)]
pub enum SpriteFileSyncMode {
    PushPullPerTask,
    PersistentWorkspace,
}

// ── VM Manager config ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VmManagerConfig {
    /// Base URL of the VM Manager API (e.g., "http://34.100.130.60:8080")
    pub api_base_url: String,
    /// Host for SSH access to VMs (defaults to host extracted from api_base_url)
    pub ssh_host: String,
    /// Default VM tier: "nano" (1 vCPU, 512MB) or "micro" (2 vCPU, 1024MB)
    pub default_tier: String,
    /// Anthropic API key to inject into VMs (optional)
    pub api_key: Option<String>,
    // TODO: auth_token for VM Manager itself (currently no auth)
}

// ── Runtime config selector ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum SandboxRuntimeConfig {
    Dangerous(DangerousConfig),
    Firecracker(FirecrackerConfig),
    VmManager(VmManagerConfig),
    FlySprite(FlySpriteConfig),
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_spec_default() {
        let fs = FilesystemSpec::default();
        assert_eq!(fs.workspace_path, "/workspace");
        assert_eq!(fs.artifacts_path, "/artifacts");
        assert_eq!(fs.tmp_path, "/tmp");
        assert!(!fs.persistent_workspace);
        assert!(fs.size_mb_hint.is_none());
    }

    #[test]
    fn resource_hints_default_is_none() {
        let r = ResourceHints::default();
        assert!(r.vcpu.is_none());
        assert!(r.memory_mb.is_none());
        assert!(r.disk_mb.is_none());
    }

    #[test]
    fn lifecycle_default_is_permissive() {
        let lp = LifecyclePolicy::default();
        assert!(lp.idle_timeout.is_none());
        assert!(!lp.auto_sleep);
        assert!(lp.ttl.is_none());
    }

    #[test]
    fn network_policy_default_safe() {
        let np = NetworkPolicy::default_safe();
        assert_eq!(np.mode, NetworkMode::Disabled);
        assert!(np.allowed_hosts.is_empty());
        assert!(np.blocked_hosts.is_empty());
        assert!(np.allow_loopback);
    }

    #[test]
    fn dangerous_config_default() {
        let dc = DangerousConfig::default();
        assert!(dc.root_dir.as_os_str().is_empty()); // must be set by caller
        assert!(dc.inherit_env_allowlist.contains(&"PATH".to_string()));
        assert!(dc.inherit_env_allowlist.contains(&"HOME".to_string()));
        assert_eq!(dc.default_shell, vec!["/bin/bash", "-lc"]);
        assert!(!dc.use_dedicated_user);
        assert!(!dc.enable_rlimits);
        assert_eq!(dc.max_output_bytes, 10 * 1024 * 1024);
    }

    #[test]
    fn sandbox_spec_builds_with_defaults() {
        let spec = SandboxSpec {
            workspace_id: "test-ws-1".into(),
            profile: SandboxProfile::Base,
            filesystem: FilesystemSpec::default(),
            resources: ResourceHints::default(),
            env: BTreeMap::new(),
            mounts: vec![],
            network: NetworkPolicy::default_safe(),
            lifecycle: LifecyclePolicy::default(),
            labels: BTreeMap::new(),
        };
        assert_eq!(spec.workspace_id, "test-ws-1");
        assert!(matches!(spec.profile, SandboxProfile::Base));
    }

    #[test]
    fn sandbox_spec_with_custom_profile_and_env() {
        let mut env = BTreeMap::new();
        env.insert("ANTHROPIC_API_KEY".into(), "sk-test".into());
        env.insert("NODE_ENV".into(), "production".into());

        let spec = SandboxSpec {
            workspace_id: "flow-42".into(),
            profile: SandboxProfile::Custom("claude-agent".into()),
            filesystem: FilesystemSpec {
                workspace_path: "/workspace".into(),
                artifacts_path: "/artifacts".into(),
                tmp_path: "/tmp".into(),
                persistent_workspace: true,
                size_mb_hint: Some(4096),
            },
            resources: ResourceHints {
                vcpu: Some(2),
                memory_mb: Some(2048),
                disk_mb: None,
            },
            env,
            mounts: vec![MountSpec {
                host_path: "/home/user/prompts".into(),
                guest_path: "/prompts".into(),
                read_only: true,
            }],
            network: NetworkPolicy {
                mode: NetworkMode::Allowlist,
                allowed_hosts: vec!["api.anthropic.com".into()],
                blocked_hosts: vec![],
                allow_loopback: true,
            },
            lifecycle: LifecyclePolicy {
                idle_timeout: Some(Duration::from_secs(30)),
                auto_sleep: true,
                ttl: Some(Duration::from_secs(7200)),
            },
            labels: BTreeMap::from([("flow".into(), "content-gather".into())]),
        };

        assert_eq!(spec.workspace_id, "flow-42");
        assert!(matches!(spec.profile, SandboxProfile::Custom(ref s) if s == "claude-agent"));
        assert!(spec.filesystem.persistent_workspace);
        assert_eq!(spec.filesystem.size_mb_hint, Some(4096));
        assert_eq!(spec.resources.vcpu, Some(2));
        assert_eq!(spec.env.len(), 2);
        assert_eq!(spec.mounts.len(), 1);
        assert!(spec.mounts[0].read_only);
        assert_eq!(spec.network.mode, NetworkMode::Allowlist);
        assert_eq!(spec.network.allowed_hosts[0], "api.anthropic.com");
        assert!(spec.lifecycle.auto_sleep);
        assert_eq!(spec.labels.get("flow").unwrap(), "content-gather");
    }

    #[test]
    fn exec_request_minimal() {
        let req = ExecRequest {
            command: vec!["echo".into(), "hello".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: None,
            tty: false,
            detach: false,
        };
        assert_eq!(req.command, vec!["echo", "hello"]);
        assert!(!req.tty);
        assert!(!req.detach);
    }

    #[test]
    fn exec_result_success() {
        let res = ExecResult {
            exit_code: Some(0),
            stdout: b"hello world\n".to_vec(),
            stderr: vec![],
            timed_out: false,
            started_at_unix_ms: 1700000000000,
            finished_at_unix_ms: Some(1700000001000),
            session_id: None,
        };
        assert_eq!(res.exit_code, Some(0));
        assert!(!res.timed_out);
        assert_eq!(String::from_utf8_lossy(&res.stdout), "hello world\n");
    }

    #[test]
    fn exec_result_timeout() {
        let res = ExecResult {
            exit_code: None,
            stdout: vec![],
            stderr: b"killed".to_vec(),
            timed_out: true,
            started_at_unix_ms: 1700000000000,
            finished_at_unix_ms: None,
            session_id: Some("sess-1".into()),
        };
        assert!(res.timed_out);
        assert!(res.exit_code.is_none());
        assert!(res.finished_at_unix_ms.is_none());
    }

    #[test]
    fn exec_event_variants() {
        let stdout = ExecEvent::Stdout(b"line1\n".to_vec());
        let stderr = ExecEvent::Stderr(b"warn\n".to_vec());
        let exit = ExecEvent::Exit { code: 0 };
        let status = ExecEvent::Status("running".into());

        assert!(matches!(stdout, ExecEvent::Stdout(_)));
        assert!(matches!(stderr, ExecEvent::Stderr(_)));
        assert!(matches!(exit, ExecEvent::Exit { code: 0 }));
        assert!(matches!(status, ExecEvent::Status(_)));
    }

    #[test]
    fn put_file_request_construction() {
        let req = PutFileRequest {
            path: "/workspace/run.sh".into(),
            bytes: b"#!/bin/bash\necho hi".to_vec(),
            create_parents: true,
            mode: Some(0o755),
        };
        assert_eq!(req.path, "/workspace/run.sh");
        assert!(req.create_parents);
        assert_eq!(req.mode, Some(0o755));
    }

    #[test]
    fn get_file_response_truncation() {
        let full = GetFileResponse {
            bytes: b"full content".to_vec(),
            truncated: false,
        };
        let truncated = GetFileResponse {
            bytes: b"partial...".to_vec(),
            truncated: true,
        };
        assert!(!full.truncated);
        assert!(truncated.truncated);
    }

    #[test]
    fn sandbox_status_variants() {
        let statuses = vec![
            SandboxStatus::Provisioning,
            SandboxStatus::Running,
            SandboxStatus::Sleeping,
            SandboxStatus::Stopped,
            SandboxStatus::Destroyed,
            SandboxStatus::Unknown,
        ];
        assert_eq!(statuses.len(), 6);
        assert_eq!(SandboxStatus::Running, SandboxStatus::Running);
        assert_ne!(SandboxStatus::Running, SandboxStatus::Stopped);
    }

    #[test]
    fn checkpoint_capability_variants() {
        assert_ne!(
            CheckpointCapability::None,
            CheckpointCapability::FilesystemOnly
        );
        assert_ne!(
            CheckpointCapability::FilesystemOnly,
            CheckpointCapability::FullVmBestEffort
        );
    }

    #[test]
    fn backend_kind_equality() {
        assert_eq!(
            SandboxBackendKind::DangerousHost,
            SandboxBackendKind::DangerousHost
        );
        assert_ne!(
            SandboxBackendKind::DangerousHost,
            SandboxBackendKind::Firecracker
        );
        assert_ne!(
            SandboxBackendKind::Firecracker,
            SandboxBackendKind::FlySprite
        );
    }

    #[test]
    fn sandbox_summary_construction() {
        let summary = SandboxSummary {
            id: "sbx-abc123".into(),
            backend: SandboxBackendKind::DangerousHost,
            status: SandboxStatus::Running,
            workspace_id: "ws-1".into(),
        };
        assert_eq!(summary.id, "sbx-abc123");
        assert_eq!(summary.backend, SandboxBackendKind::DangerousHost);
        assert_eq!(summary.status, SandboxStatus::Running);
    }

    #[test]
    fn sandbox_runtime_config_variants() {
        let dangerous = SandboxRuntimeConfig::Dangerous(DangerousConfig::default());
        assert!(matches!(dangerous, SandboxRuntimeConfig::Dangerous(_)));
    }

    #[test]
    fn types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SandboxSpec>();
        assert_send_sync::<ExecRequest>();
        assert_send_sync::<ExecResult>();
        assert_send_sync::<ExecEvent>();
        assert_send_sync::<SandboxBackendKind>();
        assert_send_sync::<SandboxStatus>();
        assert_send_sync::<DangerousConfig>();
        assert_send_sync::<NetworkPolicy>();
        assert_send_sync::<SandboxRuntimeConfig>();
    }
}
