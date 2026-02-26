use std::sync::Arc;

use crate::flows::scheduler::FlowScheduler;
use crate::flows::repository::FlowRepository;
use crate::flows::Flow;

pub struct SchedulerRepository {
    flow_repo: Arc<dyn FlowRepository>,
    scheduler: Arc<FlowScheduler>,
}

impl SchedulerRepository {
    pub fn new(flow_repo: Arc<dyn FlowRepository>, scheduler: Arc<FlowScheduler>) -> Self {
        Self { flow_repo, scheduler }
    }

    pub async fn get_flow(&self, id: &str) -> Option<Flow> {
        self.flow_repo.get_flow(id).await
    }

    pub async fn list_flows(&self) -> Vec<Flow> {
        self.flow_repo.list_flows().await
    }

    pub async fn active_flow_ids(&self) -> Vec<String> {
        self.scheduler.active_flow_ids().await
    }
}
