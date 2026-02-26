pub mod chat;
pub mod handlers;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Agent CRUD
        .route("/agents", get(handlers::list_agents).post(handlers::create_agent))
        .route(
            "/agents/{id}",
            get(handlers::get_agent)
                .put(handlers::update_agent)
                .delete(handlers::delete_agent),
        )
        // Agent chat & sessions
        .route(
            "/agents/{id}/sessions",
            get(chat::list_sessions).post(chat::new_session),
        )
        .route(
            "/agents/{id}/sessions/{session_id}",
            delete(chat::delete_session),
        )
        .route("/agents/{id}/chat", post(chat::chat))
        .route("/agents/{id}/chat/stop", post(chat::stop_chat))
}
