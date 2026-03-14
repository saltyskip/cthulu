mod handlers;

use axum::routing::{get, post};
use axum::Router;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workflows/setup", post(handlers::setup_repo))
        .route("/workflows/workspaces", get(handlers::list_workspaces))
        .route("/workflows/workspaces", post(handlers::create_workspace))
        .route(
            "/workflows/workspaces/{workspace}",
            get(handlers::list_workspace_workflows),
        )
        .route(
            "/workflows/workspaces/{workspace}/{name}",
            get(handlers::get_workflow).delete(handlers::delete_workflow),
        )
        .route(
            "/workflows/workspaces/{workspace}/{name}/save",
            post(handlers::save_workflow),
        )
        .route(
            "/workflows/workspaces/{workspace}/{name}/publish",
            post(handlers::publish_workflow),
        )
        .route("/workflows/sync", post(handlers::sync_workflows))
}
