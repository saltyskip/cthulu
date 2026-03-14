//! Minimal A2A (Agent-to-Agent) protocol client.
//!
//! Implements just enough of the A2A JSON-RPC 2.0 binding to send tasks
//! to ADK agent servers running inside cloud VMs and stream responses.
//!
//! Reference: <https://a2a-protocol.org/latest/specification/>

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

// ── A2A Protocol Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Submitted,
    Working,
    #[serde(rename = "input-required")]
    InputRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
    #[serde(rename = "auth-required")]
    AuthRequired,
}

impl TaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct A2aTask {
    pub id: String,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub artifacts: Vec<A2aArtifact>,
    #[serde(default)]
    pub history: Vec<A2aMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default)]
    pub message: Option<A2aMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aMessage {
    pub role: String, // "user" | "agent"
    pub parts: Vec<A2aPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum A2aPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "data")]
    Data { data: Value },
}

#[derive(Debug, Clone, Deserialize)]
pub struct A2aArtifact {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub parts: Vec<A2aPart>,
}

// ── JSON-RPC 2.0 envelope ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: String,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    data: Option<Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A2A RPC error {}: {}", self.code, self.message)
    }
}

// ── Client ─────────────────────────────────────────────────────────────────

/// Minimal A2A client that sends tasks to remote ADK agent servers.
#[derive(Clone)]
pub struct A2aClient {
    http: reqwest::Client,
}

impl A2aClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300)) // 5 min for long agent tasks
                .build()
                .expect("failed to build A2A HTTP client"),
        }
    }

    /// Discover a remote agent's capabilities by fetching its Agent Card.
    pub async fn get_agent_card(&self, base_url: &str) -> Result<AgentCard> {
        let url = format!(
            "{}/.well-known/agent.json",
            base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to fetch agent card from {url}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("agent card request returned {status}: {body}");
        }

        resp.json()
            .await
            .context("failed to parse agent card JSON")
    }

    /// Send a message to a remote A2A agent and wait for the task to complete.
    /// Returns the completed task with artifacts.
    pub async fn send_message(
        &self,
        agent_url: &str,
        message: &str,
        context_id: Option<&str>,
    ) -> Result<A2aTask> {
        let rpc_url = agent_url.trim_end_matches('/');
        let request_id = Uuid::new_v4().to_string();

        let mut params = json!({
            "message": {
                "role": "user",
                "parts": [
                    { "type": "text", "text": message }
                ]
            }
        });

        if let Some(ctx) = context_id {
            params["contextId"] = json!(ctx);
        }

        let rpc = JsonRpcRequest {
            jsonrpc: "2.0",
            id: request_id,
            method: "message/send".to_string(),
            params,
        };

        tracing::debug!(
            url = %rpc_url,
            message_len = message.len(),
            "sending A2A message"
        );

        let resp = self
            .http
            .post(rpc_url)
            .json(&rpc)
            .send()
            .await
            .with_context(|| format!("A2A request to {rpc_url} failed"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("A2A server returned {status}: {body}");
        }

        let rpc_resp: JsonRpcResponse = resp
            .json()
            .await
            .context("failed to parse A2A JSON-RPC response")?;

        if let Some(err) = rpc_resp.error {
            anyhow::bail!("{err}");
        }

        let result = rpc_resp
            .result
            .ok_or_else(|| anyhow::anyhow!("A2A response has neither result nor error"))?;

        // The result can be either a Task object or a Message object.
        // Try to parse as Task first.
        if let Ok(task) = serde_json::from_value::<A2aTask>(result.clone()) {
            return Ok(task);
        }

        // If it's a bare message, wrap it into a synthetic task
        if let Ok(msg) = serde_json::from_value::<A2aMessage>(result) {
            return Ok(A2aTask {
                id: Uuid::new_v4().to_string(),
                status: Some(TaskStatus {
                    state: TaskState::Completed,
                    message: Some(msg),
                }),
                artifacts: vec![],
                history: vec![],
            });
        }

        anyhow::bail!("unexpected A2A response format")
    }

    /// Get the status of an existing task.
    pub async fn get_task(&self, agent_url: &str, task_id: &str) -> Result<A2aTask> {
        let rpc_url = agent_url.trim_end_matches('/');
        let request_id = Uuid::new_v4().to_string();

        let rpc = JsonRpcRequest {
            jsonrpc: "2.0",
            id: request_id,
            method: "tasks/get".to_string(),
            params: json!({ "id": task_id }),
        };

        let resp = self
            .http
            .post(rpc_url)
            .json(&rpc)
            .send()
            .await
            .context("failed to get A2A task")?;

        let rpc_resp: JsonRpcResponse = resp
            .json()
            .await
            .context("failed to parse get-task response")?;

        if let Some(err) = rpc_resp.error {
            anyhow::bail!("{err}");
        }

        let result = rpc_resp
            .result
            .ok_or_else(|| anyhow::anyhow!("no result in get-task response"))?;

        serde_json::from_value(result).context("failed to parse task from response")
    }

    /// Extract the text content from a completed task.
    /// Looks in artifacts first, then in the status message.
    pub fn extract_text(task: &A2aTask) -> String {
        // Try artifacts first
        for artifact in &task.artifacts {
            for part in &artifact.parts {
                if let A2aPart::Text { text } = part {
                    return text.clone();
                }
            }
        }

        // Fall back to status message
        if let Some(status) = &task.status {
            if let Some(msg) = &status.message {
                for part in &msg.parts {
                    if let A2aPart::Text { text } = part {
                        return text.clone();
                    }
                }
            }
        }

        // Fall back to history (last agent message)
        for msg in task.history.iter().rev() {
            if msg.role == "agent" {
                for part in &msg.parts {
                    if let A2aPart::Text { text } = part {
                        return text.clone();
                    }
                }
            }
        }

        String::new()
    }
}

impl Default for A2aClient {
    fn default() -> Self {
        Self::new()
    }
}
