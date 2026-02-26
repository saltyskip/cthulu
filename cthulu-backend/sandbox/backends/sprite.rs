use async_trait::async_trait;

use crate::sandbox::error::SandboxError;
use crate::sandbox::handle::SandboxHandle;
use crate::sandbox::provider::SandboxProvider;
use crate::sandbox::types::*;

/// Fly Sprite cloud sandbox provider.
///
/// Phase 3+: will compose `sprite/api_client.rs`, `exec_ws.rs`,
/// `files.rs`, and `services.rs`.
pub struct FlySpriteProvider {
    #[allow(dead_code)]
    config: FlySpriteConfig,
}

impl FlySpriteProvider {
    pub fn new(config: FlySpriteConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SandboxProvider for FlySpriteProvider {
    fn info(&self) -> SandboxProviderInfo {
        SandboxProviderInfo {
            kind: SandboxBackendKind::FlySprite,
            supports_persistent_state: true,
            supports_checkpoint: true,
            supports_public_http: true,
            supports_sleep_resume: true,
        }
    }

    async fn provision(&self, _spec: SandboxSpec) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        Err(SandboxError::Unsupported(
            "fly sprite provider not yet implemented (Phase 3)",
        ))
    }

    async fn attach(&self, _id: &str) -> Result<Box<dyn SandboxHandle>, SandboxError> {
        Err(SandboxError::Unsupported(
            "fly sprite provider not yet implemented (Phase 3)",
        ))
    }

    async fn list(&self) -> Result<Vec<SandboxSummary>, SandboxError> {
        Ok(vec![])
    }
}
