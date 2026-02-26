use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::sandbox::error::SandboxError;
use crate::sandbox::handle::{ExecStream, SandboxHandle};
use crate::sandbox::local_host::fs_jail::FsJail;
use crate::sandbox::local_host::process_supervisor::ProcessSupervisor;
use crate::sandbox::provider::SandboxProvider;
use crate::sandbox::types::*;

/// Best-effort host isolation provider.
///
/// Each sandbox gets its own workspace directory under `config.root_dir`.
/// Processes run as the current user with a filtered env.
/// NOT a security boundary — for trusted workloads only.
pub struct DangerousHostProvider {
    config: DangerousConfig,
    counter: AtomicU64,
}

impl DangerousHostProvider {
    pub fn new(config: DangerousConfig) -> Result<Self, SandboxError> {
        std::fs::create_dir_all(&config.root_dir).map_err(|e| {
            SandboxError::Provision(format!(
                "failed to create sandboxes root {}: {e}",
                config.root_dir.display()
            ))
        })?;
        Ok(Self {
            config,
            counter: AtomicU64::new(0),
        })
    }
}

#[async_trait]
impl SandboxProvider for DangerousHostProvider {
    fn info(&self) -> SandboxProviderInfo {
        SandboxProviderInfo {
            kind: SandboxBackendKind::DangerousHost,
            supports_persistent_state: true,
            supports_checkpoint: false,
            supports_public_http: false,
            supports_sleep_resume: false,
        }
    }

    async fn provision(&self, spec: SandboxSpec) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        // Validate profile (best-effort: warn if expected binary is missing)
        validate_profile(&spec.profile);

        let seq = self.counter.fetch_add(1, Ordering::SeqCst);
        let sandbox_id = format!("dangerous-{}-{seq}", spec.workspace_id);
        let workspace_dir = self.config.root_dir.join(&spec.workspace_id);

        tracing::info!(
            sandbox_id = %sandbox_id,
            workspace = %spec.workspace_id,
            workspace_dir = %workspace_dir.display(),
            "provisioning DangerousHost sandbox"
        );

        let jail = FsJail::create(workspace_dir)?;
        let supervisor = ProcessSupervisor::new(
            self.config.inherit_env_allowlist.clone(),
            self.config.max_output_bytes,
        );

        let metadata = SandboxMetadata {
            workspace_id: spec.workspace_id.clone(),
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            labels: spec.labels.clone(),
        };

        tracing::info!(sandbox_id = %sandbox_id, "DangerousHost sandbox provisioned");

        Ok(Box::new(DangerousHandle {
            id: sandbox_id,
            jail: Arc::new(jail),
            supervisor: Arc::new(supervisor),
            metadata,
            env: spec.env,
        }))
    }

    async fn attach(&self, id: &str) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        // Try to derive workspace_id from sandbox id: "dangerous-{workspace_id}-{seq}"
        let workspace_id = id
            .strip_prefix("dangerous-")
            .and_then(|rest| rest.rsplit_once('-').map(|(ws, _)| ws))
            .ok_or_else(|| SandboxError::NotFound(format!("cannot parse sandbox id: {id}")))?;

        let workspace_dir = self.config.root_dir.join(workspace_id);
        let jail = FsJail::attach(workspace_dir)?;
        let supervisor = ProcessSupervisor::new(
            self.config.inherit_env_allowlist.clone(),
            self.config.max_output_bytes,
        );

        let metadata = SandboxMetadata {
            workspace_id: workspace_id.to_string(),
            created_at_unix_ms: 0, // unknown for attached
            labels: BTreeMap::new(),
        };

        Ok(Box::new(DangerousHandle {
            id: id.to_string(),
            jail: Arc::new(jail),
            supervisor: Arc::new(supervisor),
            metadata,
            env: BTreeMap::new(),
        }))
    }

    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError> {
        let mut summaries = Vec::new();
        if self.config.root_dir.exists() {
            for entry in std::fs::read_dir(&self.config.root_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    summaries.push(SandboxSummary {
                        id: format!("dangerous-{name}-0"),
                        backend: SandboxBackendKind::DangerousHost,
                        status: SandboxStatus::Running,
                        workspace_id: name,
                    });
                }
            }
        }
        Ok(summaries)
    }
}

struct DangerousHandle {
    id: String,
    jail: Arc<FsJail>,
    supervisor: Arc<ProcessSupervisor>,
    metadata: SandboxMetadata,
    /// Per-sandbox env vars (from SandboxSpec.env).
    env: BTreeMap<String, String>,
}

#[async_trait]
impl SandboxHandle for DangerousHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn backend_kind(&self) -> SandboxBackendKind {
        SandboxBackendKind::DangerousHost
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            persistent_state: true,
            checkpoint: CheckpointCapability::None,
            public_http: false,
            resumable_exec_sessions: false,
            network_policy_enforcement: false,
            host_mounts: true,
        }
    }

    fn metadata(&self) -> &SandboxMetadata {
        &self.metadata
    }

    async fn exec(&self, mut req: ExecRequest) -> Result<ExecResult, SandboxError> {
        tracing::debug!(
            sandbox_id = %self.id,
            command = ?req.command,
            "executing command in DangerousHost sandbox"
        );
        // Merge sandbox-level env into request env
        for (k, v) in &self.env {
            req.env.entry(k.clone()).or_insert_with(|| v.clone());
        }
        self.supervisor.exec(&req, self.jail.root()).await
    }

    async fn exec_stream(
        &self,
        mut req: ExecRequest,
    ) -> Result<Box<dyn ExecStream + Send + Unpin>, SandboxError> {
        for (k, v) in &self.env {
            req.env.entry(k.clone()).or_insert_with(|| v.clone());
        }
        let stream = self.supervisor.exec_stream(&req, self.jail.root()).await?;
        Ok(Box::new(stream))
    }

    async fn put_file(&self, req: PutFileRequest) -> Result<(), SandboxError> {
        self.jail.put_file(&req)
    }

    async fn get_file(&self, req: GetFileRequest) -> Result<GetFileResponse, SandboxError> {
        self.jail.get_file(&req)
    }

    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, SandboxError> {
        self.jail.read_dir(path)
    }

    async fn remove_path(&self, path: &str, recursive: bool) -> Result<(), SandboxError> {
        self.jail.remove_path(path, recursive)
    }

    async fn expose_port(&self, _req: ExposePortRequest) -> Result<ExposedEndpoint, SandboxError> {
        Err(SandboxError::Unsupported("expose_port"))
    }

    async fn unexpose_port(&self, _port: u16) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported("unexpose_port"))
    }

    async fn checkpoint(
        &self,
        _req: CheckpointRequest,
    ) -> Result<Option<CheckpointRef>, SandboxError> {
        Err(SandboxError::Unsupported("checkpoint"))
    }

    async fn restore(&self, _checkpoint_id: &str) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported("restore"))
    }

    async fn stop(&self) -> Result<(), SandboxError> {
        // No-op for dangerous host — processes aren't tracked beyond exec()
        Ok(())
    }

    async fn resume(&self) -> Result<(), SandboxError> {
        Ok(())
    }

    async fn destroy(&self) -> Result<(), SandboxError> {
        tracing::info!(sandbox_id = %self.id, "destroying DangerousHost sandbox");
        self.jail.destroy()
    }
}

/// Best-effort profile validation: log warnings if expected tools are missing.
fn validate_profile(profile: &SandboxProfile) {
    let bins_to_check: &[&str] = match profile {
        SandboxProfile::Base => &[],
        SandboxProfile::PythonAgent => &["python3"],
        SandboxProfile::NodeAgent => &["node"],
        SandboxProfile::RustAgent => &["rustc", "cargo"],
        SandboxProfile::FullDev => &["python3", "node", "rustc"],
        SandboxProfile::Custom(_) => &[],
    };

    for bin in bins_to_check {
        if which(bin).is_none() {
            tracing::warn!(
                binary = bin,
                "sandbox profile requires '{bin}' but it was not found on PATH"
            );
        }
    }
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(bin);
            if full.is_file() { Some(full) } else { None }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_provider() -> (tempfile::TempDir, DangerousHostProvider) {
        let tmp = tempfile::tempdir().unwrap();
        let config = DangerousConfig {
            root_dir: tmp.path().join("sandboxes"),
            ..DangerousConfig::default()
        };
        let provider = DangerousHostProvider::new(config).unwrap();
        (tmp, provider)
    }

    fn base_spec(workspace_id: &str) -> SandboxSpec {
        SandboxSpec {
            workspace_id: workspace_id.into(),
            profile: SandboxProfile::Base,
            filesystem: FilesystemSpec::default(),
            resources: ResourceHints::default(),
            env: BTreeMap::new(),
            mounts: vec![],
            network: NetworkPolicy::default_safe(),
            lifecycle: LifecyclePolicy::default(),
            labels: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn provision_and_exec() {
        let (_tmp, provider) = test_provider().await;
        let handle = provider.provision(base_spec("test-ws")).await.unwrap();

        assert!(handle.id().starts_with("dangerous-"));
        assert_eq!(handle.backend_kind(), SandboxBackendKind::DangerousHost);

        let result = handle
            .exec(ExecRequest {
                command: vec!["echo".into(), "from sandbox".into()],
                cwd: None,
                env: BTreeMap::new(),
                stdin: None,
                timeout: Some(std::time::Duration::from_secs(5)),
                tty: false,
                detach: false,
            })
            .await
            .unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            "from sandbox"
        );
    }

    #[tokio::test]
    async fn provision_put_get_file() {
        let (_tmp, provider) = test_provider().await;
        let handle = provider.provision(base_spec("files-ws")).await.unwrap();

        handle
            .put_file(PutFileRequest {
                path: "/test.txt".into(),
                bytes: b"sandbox file".to_vec(),
                create_parents: false,
                mode: None,
            })
            .await
            .unwrap();

        let resp = handle
            .get_file(GetFileRequest {
                path: "/test.txt".into(),
                max_bytes: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.bytes, b"sandbox file");
    }

    #[tokio::test]
    async fn provision_with_env() {
        let (_tmp, provider) = test_provider().await;
        let mut env = BTreeMap::new();
        env.insert("MY_SECRET".into(), "s3cret".into());

        let mut spec = base_spec("env-ws");
        spec.env = env;

        let handle = provider.provision(spec).await.unwrap();
        let result = handle
            .exec(ExecRequest {
                command: vec!["bash".into(), "-c".into(), "echo $MY_SECRET".into()],
                cwd: None,
                env: BTreeMap::new(),
                stdin: None,
                timeout: Some(std::time::Duration::from_secs(5)),
                tty: false,
                detach: false,
            })
            .await
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            "s3cret"
        );
    }

    #[tokio::test]
    async fn unsupported_operations() {
        let (_tmp, provider) = test_provider().await;
        let handle = provider.provision(base_spec("unsup-ws")).await.unwrap();

        assert!(matches!(
            handle.expose_port(ExposePortRequest {
                port: 8080,
                protocol: PortProtocol::Http,
                visibility: PortVisibility::Public,
            }).await,
            Err(SandboxError::Unsupported("expose_port"))
        ));

        assert!(matches!(
            handle.checkpoint(CheckpointRequest {
                name: None,
                include_memory_state: false,
            }).await,
            Err(SandboxError::Unsupported("checkpoint"))
        ));

        assert!(matches!(
            handle.restore("cp-1").await,
            Err(SandboxError::Unsupported("restore"))
        ));
    }

    #[tokio::test]
    async fn destroy_removes_workspace() {
        let (_tmp, provider) = test_provider().await;
        let handle = provider.provision(base_spec("destroy-ws")).await.unwrap();

        handle
            .put_file(PutFileRequest {
                path: "/file.txt".into(),
                bytes: b"data".to_vec(),
                create_parents: false,
                mode: None,
            })
            .await
            .unwrap();

        handle.destroy().await.unwrap();

        // Verify workspace is gone
        let list = provider.list().await.unwrap();
        assert!(!list.iter().any(|s| s.workspace_id == "destroy-ws"));
    }

    #[tokio::test]
    async fn list_sandboxes() {
        let (_tmp, provider) = test_provider().await;
        provider.provision(base_spec("ws-a")).await.unwrap();
        provider.provision(base_spec("ws-b")).await.unwrap();

        let list = provider.list().await.unwrap();
        assert_eq!(list.len(), 2);
        let ids: Vec<_> = list.iter().map(|s| s.workspace_id.as_str()).collect();
        assert!(ids.contains(&"ws-a"));
        assert!(ids.contains(&"ws-b"));
    }

    #[tokio::test]
    async fn provider_info() {
        let (_tmp, provider) = test_provider().await;
        let info = provider.info();
        assert_eq!(info.kind, SandboxBackendKind::DangerousHost);
        assert!(info.supports_persistent_state);
        assert!(!info.supports_checkpoint);
        assert!(!info.supports_public_http);
    }

    #[tokio::test]
    async fn capabilities() {
        let (_tmp, provider) = test_provider().await;
        let handle = provider.provision(base_spec("cap-ws")).await.unwrap();
        let caps = handle.capabilities();
        assert!(caps.persistent_state);
        assert_eq!(caps.checkpoint, CheckpointCapability::None);
        assert!(!caps.public_http);
        assert!(caps.host_mounts);
    }
}
