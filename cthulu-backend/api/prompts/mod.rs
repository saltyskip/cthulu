pub mod handlers;

use axum::routing::{get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/prompts", get(handlers::list_prompts).post(handlers::create_prompt))
        .route(
            "/prompts/{id}",
            get(handlers::get_prompt)
                .put(handlers::update_prompt)
                .delete(handlers::delete_prompt),
        )
        .route("/prompts/summarize", post(handlers::summarize_session))
}
