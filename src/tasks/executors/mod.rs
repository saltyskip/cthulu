pub mod claude_code;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<()>;
}
