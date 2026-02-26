use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RunEvent {
    pub flow_id: String,
    pub run_id: String,
    pub timestamp: DateTime<Utc>,
    pub node_id: Option<String>,
    pub event_type: RunEventType,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventType {
    RunStarted,
    NodeStarted,
    NodeCompleted,
    NodeFailed,
    RunCompleted,
    RunFailed,
    Log,
}

impl RunEventType {
    pub fn as_sse_event(&self) -> &'static str {
        match self {
            RunEventType::RunStarted => "run_started",
            RunEventType::NodeStarted => "node_started",
            RunEventType::NodeCompleted => "node_completed",
            RunEventType::NodeFailed => "node_failed",
            RunEventType::RunCompleted => "run_completed",
            RunEventType::RunFailed => "run_failed",
            RunEventType::Log => "log",
        }
    }
}
