pub mod handlers;
pub mod repository;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/token-status", get(handlers::token_status))
        .route("/auth/refresh-token", post(handlers::refresh_token))
}
