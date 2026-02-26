pub mod handlers;
pub mod repository;

use axum::routing::get;
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sandbox/info", get(handlers::sandbox_info))
        .route("/sandbox/list", get(handlers::sandbox_list))
        .route(
            "/sandbox/vm/{flow_id}/{node_id}",
            get(handlers::get_node_vm)
                .post(handlers::create_node_vm)
                .delete(handlers::delete_node_vm),
        )
}
