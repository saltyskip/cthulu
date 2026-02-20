pub mod flow_routes;
pub mod middleware;
pub mod routes;

use axum::Router;
use tokio::sync::broadcast;

use crate::flows::events::RunEvent;
use crate::flows::scheduler::FlowScheduler;
use crate::flows::store::Store;
use crate::github::client::GithubClient;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub http_client: Arc<reqwest::Client>,
    pub store: Arc<dyn Store>,
    pub scheduler: Arc<FlowScheduler>,
    pub events_tx: broadcast::Sender<RunEvent>,
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}
