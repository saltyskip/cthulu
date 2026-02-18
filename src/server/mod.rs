pub mod middleware;
pub mod routes;

use axum::Router;

use crate::github::client::GithubClient;
use crate::relay;
use crate::tasks::TaskState;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub task_state: Arc<TaskState>,
    pub config: Arc<crate::config::Config>,
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub http_client: Arc<reqwest::Client>,
    // Slack interactive relay
    pub bot_user_id: Arc<RwLock<Option<String>>>,
    pub thread_sessions: relay::ThreadSessions,
    pub seen_event_ids: Arc<RwLock<VecDeque<String>>>,
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}
