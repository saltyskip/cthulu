pub mod chat;
pub mod handlers;
pub mod terminal;

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
        .route(
            "/agents/{id}/sessions/{session_id}/status",
            get(chat::session_status),
        )
        .route(
            "/agents/{id}/sessions/{session_id}/kill",
            post(chat::kill_session),
        )
        .route(
            "/agents/{id}/sessions/{session_id}/stream",
            get(chat::stream_session_log),
        )
        .route(
            "/agents/{id}/sessions/{session_id}/chat/stream",
            get(chat::stream_agent_chat),
        )
        .route(
            "/agents/{id}/sessions/{session_id}/log",
            get(chat::get_session_log),
        )
        .route("/agents/{id}/chat", post(chat::chat))
        .route("/agents/{id}/chat/stop", post(chat::stop_chat))
        // PTY terminal WebSocket
        .route("/agents/{id}/terminal", get(terminal::terminal_ws))
}
