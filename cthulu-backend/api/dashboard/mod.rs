pub mod handlers;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard/tasks", get(handlers::list_tasks))
        .route("/dashboard/tasks", post(handlers::save_task))
        .route("/dashboard/tasks/{id}", delete(handlers::delete_task))
        .route("/dashboard/run", post(handlers::run_task))
        .route("/dashboard/extract-todos", post(handlers::extract_todos))
        .route("/dashboard/history", get(handlers::get_history))
}
