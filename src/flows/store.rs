use anyhow::Result;
use async_trait::async_trait;

use super::Flow;
use super::history::{FlowRun, NodeRun, RunStatus};

#[async_trait]
pub trait Store: Send + Sync {
    // Flows
    async fn list_flows(&self) -> Vec<Flow>;
    async fn get_flow(&self, id: &str) -> Option<Flow>;
    async fn save_flow(&self, flow: Flow) -> Result<()>;
    async fn delete_flow(&self, id: &str) -> Result<bool>;

    // Runs
    async fn add_run(&self, run: FlowRun) -> Result<()>;
    async fn get_runs(&self, flow_id: &str, limit: usize) -> Vec<FlowRun>;
    async fn complete_run(
        &self,
        flow_id: &str,
        run_id: &str,
        status: RunStatus,
        error: Option<String>,
    ) -> Result<()>;
    async fn push_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_run: NodeRun,
    ) -> Result<()>;
    async fn complete_node_run(
        &self,
        flow_id: &str,
        run_id: &str,
        node_id: &str,
        status: RunStatus,
        output_preview: Option<String>,
    ) -> Result<()>;

    // Lifecycle
    async fn load_all(&self) -> Result<()>;
}
