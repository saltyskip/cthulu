pub mod claude_code;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub text: String,
    pub cost_usd: f64,
    pub num_turns: u64,
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<ExecutionResult>;
}
