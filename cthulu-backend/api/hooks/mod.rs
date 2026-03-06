pub mod routes;

use axum::routing::post;
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/hooks/pre-tool-use", post(routes::pre_tool_use))
        .route("/hooks/post-tool-use", post(routes::post_tool_use))
        .route("/hooks/stop", post(routes::stop))
        .route(
            "/hooks/permission-response",
            post(routes::permission_response),
        )
}
