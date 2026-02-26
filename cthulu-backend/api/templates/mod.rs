pub mod handlers;
pub mod repository;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/templates", get(handlers::list_templates))
        .route("/templates/import-yaml", post(handlers::import_yaml))
        .route("/templates/import-github", post(handlers::import_github))
        .route("/templates/{category}/{slug}", get(handlers::get_template_yaml))
        .route("/templates/{category}/{slug}/import", post(handlers::import_template))
}
