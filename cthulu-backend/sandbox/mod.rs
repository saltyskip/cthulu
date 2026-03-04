pub mod backends;
pub mod error;
pub mod handle;
pub mod local_host;
pub mod provider;
pub mod types;

// Internal implementation modules
pub mod firecracker;
#[allow(dead_code)]
mod sprite;

pub use error::SandboxError;
pub use handle::{ExecStream, SandboxHandle};
pub use provider::SandboxProvider;
pub use types::*;

use backends::dangerous::DangerousHostProvider;
use backends::firecracker::FirecrackerProvider;

/// Build a sandbox provider from runtime config.
pub fn build_provider(
    config: SandboxRuntimeConfig,
) -> Result<Box<dyn SandboxProvider>, SandboxError> {
    match config {
        SandboxRuntimeConfig::Dangerous(c) => {
            tracing::info!("initializing DangerousHost sandbox provider");
            Ok(Box::new(DangerousHostProvider::new(c)?))
        }
        SandboxRuntimeConfig::Firecracker(c) => {
            tracing::info!(
                state_dir = %c.state_dir.display(),
                "initializing Firecracker sandbox provider"
            );
            Ok(Box::new(FirecrackerProvider::new(c)?))
        }
        SandboxRuntimeConfig::FlySprite(_) => Err(SandboxError::Unsupported(
            "fly sprite backend not yet implemented",
        )),
    }
}
