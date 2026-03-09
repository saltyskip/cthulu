use crate::agent_sdk::config::SessionConfig;
use crate::agent_sdk::message::{self, SseEvent};

use anyhow::{Context, Result, bail};
use claude_agent_sdk_rust::ClaudeSDKClient;
use futures::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// A live agent session backed by `ClaudeSDKClient`.
/// Stored in `AppState.sdk_sessions` keyed by process_key.
pub struct AgentSession {
    client: Option<ClaudeSDKClient>,
    session_id: Option<String>,
    busy: bool,
}

impl AgentSession {
    pub async fn create(
        config: SessionConfig,
        agent_id: &str,
        session_id: &str,
    ) -> Result<Self> {
        let sdk_options = config.into_sdk();
        let mut client = ClaudeSDKClient::new(sdk_options);

        client
            .connect(None)
            .await
            .context("failed to connect to Claude Code CLI")?;

        info!(agent_id, session_id, "agent session created via SDK");

        Ok(Self {
            client: Some(client),
            session_id: None,
            busy: false,
        })
    }

    /// Send a user message and stream SSE events to a broadcast channel.
    /// Drives the SDK response stream internally — does not return until
    /// the response is complete.
    pub async fn send_message(
        &mut self,
        prompt: &str,
    ) -> Result<broadcast::Receiver<SseEvent>> {
        if self.client.is_none() {
            bail!("session not connected");
        }
        if self.busy {
            bail!("session is busy processing a previous message");
        }

        self.busy = true;

        let client = self.client.as_mut().unwrap();

        client
            .query(prompt)
            .await
            .context("failed to send message to Claude")?;

        let (tx, rx) = broadcast::channel::<SseEvent>(1024);
        let mut captured_session_id: Option<String> = None;

        {
            let response_stream = client
                .receive_response()
                .context("failed to receive response stream")?;

            let mut stream = Box::pin(response_stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => {
                        if captured_session_id.is_none() {
                            captured_session_id = message::extract_session_id(&msg);
                        }

                        let events = message::sdk_message_to_sse_events(&msg);
                        for event in events {
                            if tx.send(event).is_err() {
                                debug!("all SSE receivers dropped, stopping stream");
                                break;
                            }
                        }

                        if msg.is_result() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "error reading SDK message");
                        let _ = tx.send(SseEvent {
                            event_type: "error".to_string(),
                            data: serde_json::json!({ "error": e.to_string() }),
                        });
                        break;
                    }
                }
            }
        }

        let _ = tx.send(SseEvent {
            event_type: "done".to_string(),
            data: serde_json::json!({}),
        });

        if self.session_id.is_none() {
            self.session_id = captured_session_id;
        }
        self.busy = false;

        Ok(rx)
    }

    pub fn is_connected(&self) -> bool {
        self.client
            .as_ref()
            .map_or(false, |c| c.is_connected())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            client
                .disconnect()
                .await
                .context("failed to disconnect session")?;
        }
        Ok(())
    }
}

impl Drop for AgentSession {
    fn drop(&mut self) {
        if self.client.as_ref().map_or(false, |c| c.is_connected()) {
            warn!("AgentSession dropped while still connected — process may leak");
        }
    }
}
