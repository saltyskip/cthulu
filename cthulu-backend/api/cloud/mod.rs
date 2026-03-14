mod handlers;

use axum::routing::{get, post};
use axum::Router;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/cloud/pool", get(handlers::pool_status))
        .route("/cloud/pool/health", get(handlers::pool_health))
        .route("/cloud/pool/test", post(handlers::test_agent))
}
