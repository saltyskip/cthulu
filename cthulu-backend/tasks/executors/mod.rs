pub mod claude_code;
pub mod sandbox;
pub mod vm_executor;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub text: String,
    pub cost_usd: f64,
    pub num_turns: u64,
}

/// Callback that receives each stdout line from the executor process.
pub type LineSink = Arc<dyn Fn(String) + Send + Sync>;

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<ExecutionResult>;

    /// Execute with optional line-by-line streaming callback.
    /// Default implementation delegates to `execute()`.
    async fn execute_streaming(
        &self,
        prompt: &str,
        working_dir: &Path,
        _line_sink: Option<LineSink>,
    ) -> Result<ExecutionResult> {
        self.execute(prompt, working_dir).await
    }
}
