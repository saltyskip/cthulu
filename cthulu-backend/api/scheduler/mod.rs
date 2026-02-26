pub mod handlers;
pub mod repository;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/flows/{id}/schedule", get(handlers::get_schedule))
        .route("/scheduler/status", get(handlers::scheduler_status))
        .route("/validate/cron", post(handlers::validate_cron))
}
