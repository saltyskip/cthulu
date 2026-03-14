mod handlers;

use axum::routing::{get, post};
use axum::Router;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/secrets/github-pat", get(handlers::get_github_pat_status))
        .route("/secrets/github-pat", post(handlers::save_github_pat))
}
