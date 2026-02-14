pub mod middleware;
pub mod routes;

use axum::Router;

use crate::github::client::GithubClient;
use crate::tasks::TaskState;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub task_state: Arc<TaskState>,
    pub config: Arc<crate::config::Config>,
    pub github_client: Option<Arc<dyn GithubClient>>,
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}
