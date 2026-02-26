use anyhow::Result;
use async_trait::async_trait;

use super::Agent;

#[async_trait]
pub trait AgentRepository: Send + Sync {
    async fn list(&self) -> Vec<Agent>;
    async fn get(&self, id: &str) -> Option<Agent>;
    async fn save(&self, agent: Agent) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<bool>;
    async fn load_all(&self) -> Result<()>;
}
