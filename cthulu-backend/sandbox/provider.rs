use async_trait::async_trait;

use super::error::SandboxError;
use super::handle::SandboxHandle;
use super::types::{SandboxProviderInfo, SandboxSpec, SandboxSummary};

/// Factory for creating and managing sandboxes.
///
/// One provider instance per backend kind lives on `AppState`.
/// Each call to `provision()` creates a new isolated sandbox.
#[async_trait]
pub trait SandboxProvider: Send + Sync {
    /// Metadata about this provider (backend kind, capability flags).
    fn info(&self) -> SandboxProviderInfo;

    /// Create a new sandbox from the given spec.
    async fn provision(&self, spec: SandboxSpec) -> Result<Box<dyn SandboxHandle>, SandboxError>;

    /// Attach to an existing sandbox by id (for resume / recovery).
    async fn attach(&self, id: &str) -> Result<Box<dyn SandboxHandle>, SandboxError>;

    /// List active/known sandboxes for this provider.
    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError>;
}
