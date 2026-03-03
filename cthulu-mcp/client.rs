//! CthuluClient — thin async HTTP client over the Cthulu REST API.
//!
//! All methods return `anyhow::Result<serde_json::Value>` — raw JSON is passed
//! directly to MCP tool responses without re-serialisation, keeping this layer
//! dependency-free of cthulu-backend's domain types.

use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct CthuluClient {
    base_url: String,
    http: reqwest::Client,
}

impl CthuluClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120)) // generous timeout for chat_with_agent
            .build()
            .expect("failed to build HTTP client");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.http
            .get(self.url(path))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error on GET {path}"))?
            .json()
            .await
            .with_context(|| format!("parse JSON from GET {path}"))
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.http
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error on POST {path}"))?
            .json()
            .await
            .with_context(|| format!("parse JSON from POST {path}"))
    }

    async fn put(&self, path: &str, body: Value) -> Result<Value> {
        self.http
            .put(self.url(path))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("PUT {path}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error on PUT {path}"))?
            .json()
            .await
            .with_context(|| format!("parse JSON from PUT {path}"))
    }

    async fn delete(&self, path: &str) -> Result<Value> {
        let resp = self
            .http
            .delete(self.url(path))
            .send()
            .await
            .with_context(|| format!("DELETE {path}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error on DELETE {path}"))?;

        // Some DELETE endpoints return 204 No Content
        if resp.content_length().map(|l| l == 0).unwrap_or(false)
            || resp.status() == reqwest::StatusCode::NO_CONTENT
        {
            return Ok(Value::Null);
        }
        resp.json()
            .await
            .with_context(|| format!("parse JSON from DELETE {path}"))
    }

    // ── Health ────────────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn health(&self) -> Result<Value> {
        self.get("/health").await
    }

    // ── Flows ─────────────────────────────────────────────────────────────────

    pub async fn list_flows(&self) -> Result<Value> {
        self.get("/api/flows").await
    }

    pub async fn get_flow(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/flows/{id}")).await
    }

    pub async fn create_flow(&self, body: Value) -> Result<Value> {
        self.post("/api/flows", body).await
    }

    pub async fn update_flow(&self, id: &str, body: Value) -> Result<Value> {
        self.put(&format!("/api/flows/{id}"), body).await
    }

    pub async fn delete_flow(&self, id: &str) -> Result<Value> {
        self.delete(&format!("/api/flows/{id}")).await
    }

    pub async fn trigger_flow(&self, id: &str, body: Option<Value>) -> Result<Value> {
        self.post(
            &format!("/api/flows/{id}/trigger"),
            body.unwrap_or(Value::Null),
        )
        .await
    }

    pub async fn get_flow_runs(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/flows/{id}/runs")).await
    }

    pub async fn get_flow_schedule(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/flows/{id}/schedule")).await
    }

    pub async fn list_node_types(&self) -> Result<Value> {
        self.get("/api/node-types").await
    }

    // ── Agents ────────────────────────────────────────────────────────────────

    pub async fn list_agents(&self) -> Result<Value> {
        self.get("/api/agents").await
    }

    pub async fn get_agent(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/agents/{id}")).await
    }

    pub async fn create_agent(&self, body: Value) -> Result<Value> {
        self.post("/api/agents", body).await
    }

    pub async fn update_agent(&self, id: &str, body: Value) -> Result<Value> {
        self.put(&format!("/api/agents/{id}"), body).await
    }

    pub async fn delete_agent(&self, id: &str) -> Result<Value> {
        self.delete(&format!("/api/agents/{id}")).await
    }

    pub async fn list_agent_sessions(&self, agent_id: &str) -> Result<Value> {
        self.get(&format!("/api/agents/{agent_id}/sessions")).await
    }

    pub async fn create_agent_session(&self, agent_id: &str) -> Result<Value> {
        self.post(&format!("/api/agents/{agent_id}/sessions"), Value::Null)
            .await
    }

    pub async fn delete_agent_session(&self, agent_id: &str, session_id: &str) -> Result<Value> {
        self.delete(&format!("/api/agents/{agent_id}/sessions/{session_id}"))
            .await
    }

    pub async fn get_session_status(&self, agent_id: &str, session_id: &str) -> Result<Value> {
        self.get(&format!(
            "/api/agents/{agent_id}/sessions/{session_id}/status"
        ))
        .await
    }

    #[allow(dead_code)]
    pub async fn kill_session(&self, agent_id: &str, session_id: &str) -> Result<Value> {
        self.post(
            &format!("/api/agents/{agent_id}/sessions/{session_id}/kill"),
            Value::Null,
        )
        .await
    }

    pub async fn get_session_log(&self, agent_id: &str, session_id: &str) -> Result<Value> {
        // The log endpoint returns JSONL (one JSON object per line), not a JSON array.
        // We fetch as text and return as a JSON string so tools can present it cleanly.
        let text = self
            .http
            .get(self.url(&format!(
                "/api/agents/{agent_id}/sessions/{session_id}/log"
            )))
            .send()
            .await
            .with_context(|| "GET session log")?
            .error_for_status()
            .with_context(|| "HTTP error on GET session log")?
            .text()
            .await
            .with_context(|| "read session log body")?;

        Ok(Value::String(text))
    }

    /// Send a message to an agent and poll until the response is ready.
    /// Returns the last assistant turn as a string.
    pub async fn chat_with_agent(
        &self,
        agent_id: &str,
        session_id: &str,
        message: &str,
    ) -> Result<String> {
        // POST the message — this starts the Claude process.
        // The endpoint returns an SSE stream. We must keep the connection alive
        // (consume the stream in a background task) so the backend doesn't detect
        // a client disconnect and cancel the Claude process.
        let chat_url = self.url(&format!("/api/agents/{agent_id}/chat"));
        let resp = self
            .http
            .post(&chat_url)
            .json(&serde_json::json!({
                "message": message,
                "session_id": session_id
            }))
            .send()
            .await
            .with_context(|| "POST agent chat")?;

        // Drain the SSE stream in the background to keep the connection open.
        // We don't need the events — we poll status separately — but dropping
        // the response would close the connection and may abort the backend task.
        tokio::spawn(async move {
            let _ = resp.bytes().await;
        });

        // Poll status every 500 ms for up to 120 s
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let status = self.get_session_status(agent_id, session_id).await?;
            let busy = status
                .get("busy")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if !busy {
                break;
            }

            if std::time::Instant::now() >= deadline {
                return Err(anyhow::anyhow!(
                    "chat_with_agent timed out after 120 s — session may still be running"
                ));
            }
        }

        // Retrieve the full log and extract the last assistant message
        let log_text = self
            .get_session_log(agent_id, session_id)
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(extract_last_assistant_turn(&log_text))
    }

    // ── Prompts ───────────────────────────────────────────────────────────────

    pub async fn list_prompts(&self) -> Result<Value> {
        self.get("/api/prompts").await
    }

    pub async fn get_prompt(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/prompts/{id}")).await
    }

    pub async fn create_prompt(&self, body: Value) -> Result<Value> {
        self.post("/api/prompts", body).await
    }

    pub async fn update_prompt(&self, id: &str, body: Value) -> Result<Value> {
        self.put(&format!("/api/prompts/{id}"), body).await
    }

    pub async fn delete_prompt(&self, id: &str) -> Result<Value> {
        self.delete(&format!("/api/prompts/{id}")).await
    }

    // ── Templates ─────────────────────────────────────────────────────────────

    pub async fn list_templates(&self) -> Result<Value> {
        self.get("/api/templates").await
    }

    pub async fn import_template(&self, category: &str, slug: &str) -> Result<Value> {
        self.post(
            &format!("/api/templates/{category}/{slug}/import"),
            Value::Null,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn import_yaml(&self, yaml: &str) -> Result<Value> {
        self.post(
            "/api/templates/import-yaml",
            serde_json::json!({ "yaml": yaml }),
        )
        .await
    }

    // ── Scheduler ─────────────────────────────────────────────────────────────

    pub async fn get_scheduler_status(&self) -> Result<Value> {
        self.get("/api/scheduler/status").await
    }

    pub async fn validate_cron(&self, expression: &str) -> Result<Value> {
        self.post(
            "/api/validate/cron",
            serde_json::json!({ "expression": expression }),
        )
        .await
    }

    // ── Auth ──────────────────────────────────────────────────────────────────

    pub async fn get_token_status(&self) -> Result<Value> {
        self.get("/api/auth/token-status").await
    }

    #[allow(dead_code)]
    pub async fn refresh_token(&self) -> Result<Value> {
        self.post("/api/auth/refresh-token", Value::Null).await
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse JSONL session log and extract the text content of the last assistant turn.
fn extract_last_assistant_turn(jsonl: &str) -> String {
    let mut last_assistant = String::new();

    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(obj) = serde_json::from_str::<Value>(line) {
            // Look for assistant message events
            // The claude stream-json format uses {"type":"assistant","message":{...}}
            // or {"role":"assistant","content":[...]}
            let is_assistant = obj
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "assistant")
                .unwrap_or(false)
                || obj
                    .get("role")
                    .and_then(|v| v.as_str())
                    .map(|r| r == "assistant")
                    .unwrap_or(false);

            if is_assistant {
                // Extract text from content array
                if let Some(content) = obj
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .or_else(|| obj.get("content"))
                {
                    let text = if content.is_array() {
                        content
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(|block| {
                                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if content.is_string() {
                        content.as_str().unwrap_or("").to_string()
                    } else {
                        continue;
                    };

                    if !text.is_empty() {
                        last_assistant = text;
                    }
                }
            }
        }
    }

    if last_assistant.is_empty() {
        "(No assistant response found in session log)".to_string()
    } else {
        last_assistant
    }
}
