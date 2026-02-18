pub mod flow_routes;
pub mod middleware;
pub mod routes;

use axum::Router;

use crate::flows::history::RunHistory;
use crate::flows::storage::FlowStore;
use crate::github::client::GithubClient;
use crate::tasks::TaskState;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub task_state: Arc<TaskState>,
    pub config: Arc<crate::config::Config>,
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub http_client: Arc<reqwest::Client>,
    pub flow_store: Arc<FlowStore>,
    pub run_history: Arc<RunHistory>,
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}
