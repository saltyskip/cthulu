use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

use super::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChangeEvent {
    pub resource_type: ResourceType,
    pub change_type: ChangeType,
    pub resource_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Flow,
    Agent,
    Prompt,
}

impl ResourceType {
    pub fn as_sse_event(self) -> &'static str {
        match self {
            ResourceType::Flow => "flow_change",
            ResourceType::Agent => "agent_change",
            ResourceType::Prompt => "prompt_change",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Updated,
    Deleted,
}

pub(crate) async fn stream_changes(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.changes_tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let sse_event_name = event.resource_type.as_sse_event();
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().event(sse_event_name).data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "changes SSE subscriber lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/changes", get(stream_changes))
}
