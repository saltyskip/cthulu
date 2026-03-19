pub mod handlers;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard/config", get(handlers::get_config))
        .route("/dashboard/config", post(handlers::save_config))
        .route("/dashboard/messages", get(handlers::get_messages))
        .route("/dashboard/summary", post(handlers::generate_summary))
}
