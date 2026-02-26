pub mod notion;
pub mod slack;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Sink: Send + Sync {
    async fn deliver(&self, text: &str) -> Result<()>;
}
