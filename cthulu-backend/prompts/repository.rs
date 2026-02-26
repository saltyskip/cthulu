use anyhow::Result;
use async_trait::async_trait;

use super::SavedPrompt;

#[async_trait]
pub trait PromptRepository: Send + Sync {
    async fn list_prompts(&self) -> Vec<SavedPrompt>;
    async fn get_prompt(&self, id: &str) -> Option<SavedPrompt>;
    async fn save_prompt(&self, prompt: SavedPrompt) -> Result<()>;
    async fn delete_prompt(&self, id: &str) -> Result<bool>;
    async fn load_all(&self) -> Result<()>;
}
