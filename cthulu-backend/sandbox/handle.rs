use async_trait::async_trait;

use super::error::SandboxError;
use super::types::*;

/// Handle to a running sandbox. Everything the agent runner needs.
///
/// Backends implement this trait. Operations that a backend does not
/// support should return `SandboxError::Unsupported`.
#[async_trait]
pub trait SandboxHandle: Send + Sync {
    fn id(&self) -> &str;
    fn backend_kind(&self) -> SandboxBackendKind;
    fn capabilities(&self) -> SandboxCapabilities;
    fn metadata(&self) -> &SandboxMetadata;

    // ── Exec ────────────────────────────────────────────────────

    /// Run a command to completion, capturing stdout/stderr.
    async fn exec(&self, req: ExecRequest) -> Result<ExecResult, SandboxError>;

    /// Streaming exec for long-running / interactive commands.
    async fn exec_stream(
        &self,
        req: ExecRequest,
    ) -> Result<Box<dyn ExecStream + Send + Unpin>, SandboxError>;

    // ── Files ───────────────────────────────────────────────────

    async fn put_file(&self, req: PutFileRequest) -> Result<(), SandboxError>;
    async fn get_file(&self, req: GetFileRequest) -> Result<GetFileResponse, SandboxError>;
    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, SandboxError>;
    async fn remove_path(&self, path: &str, recursive: bool) -> Result<(), SandboxError>;

    // ── Ports (stub for most backends) ──────────────────────────

    async fn expose_port(&self, req: ExposePortRequest) -> Result<ExposedEndpoint, SandboxError>;
    async fn unexpose_port(&self, port: u16) -> Result<(), SandboxError>;

    // ── Checkpoints (stub for most backends) ────────────────────

    async fn checkpoint(
        &self,
        req: CheckpointRequest,
    ) -> Result<Option<CheckpointRef>, SandboxError>;
    async fn restore(&self, checkpoint_id: &str) -> Result<(), SandboxError>;

    // ── Lifecycle ───────────────────────────────────────────────

    async fn stop(&self) -> Result<(), SandboxError>;
    async fn resume(&self) -> Result<(), SandboxError>;
    async fn destroy(&self) -> Result<(), SandboxError>;
}

/// Stream of events from a running exec session.
#[async_trait]
pub trait ExecStream: Send {
    async fn next_event(&mut self) -> Result<Option<ExecEvent>, SandboxError>;
    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), SandboxError>;
    async fn close_stdin(&mut self) -> Result<(), SandboxError>;
}
