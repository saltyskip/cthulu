pub mod handlers;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Flow CRUD
        .route("/flows", get(handlers::list_flows).post(handlers::create_flow))
        .route(
            "/flows/{id}",
            get(handlers::get_flow)
                .put(handlers::update_flow)
                .delete(handlers::delete_flow),
        )
        .route("/flows/{id}/trigger", post(handlers::trigger_flow))
        .route("/flows/{id}/runs", get(handlers::get_runs))
        .route("/flows/{id}/runs/live", get(handlers::stream_runs))
        .route("/node-types", get(handlers::get_node_types))
        .route("/prompt-files", get(handlers::list_prompt_files))
}
