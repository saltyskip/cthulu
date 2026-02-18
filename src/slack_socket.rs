use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use crate::relay;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_BACKOFF_SECS: u64 = 8;

// ---------------------------------------------------------------------------
// Public entry point — runs forever, reconnecting on close/error
// ---------------------------------------------------------------------------

pub async fn run(state: AppState) {
    let app_token = match state.config.slack_app_token() {
        Some(t) => t,
        None => {
            tracing::warn!("No SLACK_APP_TOKEN set, Socket Mode disabled");
            return;
        }
    };

    let mut backoff_secs: u64 = 1;

    loop {
        tracing::info!("Connecting to Slack Socket Mode...");

        match open_connection(&state.http_client, &app_token).await {
            Ok(wss_url) => {
                backoff_secs = 1; // reset on successful connection open
                match run_ws_loop(&state, &wss_url).await {
                    Ok(reason) => {
                        tracing::info!(reason, "WebSocket closed, reconnecting...");
                        backoff_secs = 1;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "WebSocket error, reconnecting...");
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to open Socket Mode connection");
            }
        }

        tracing::info!(backoff_secs, "Waiting before reconnect...");
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

// ---------------------------------------------------------------------------
// Open connection: POST apps.connections.open to get WSS URL
// ---------------------------------------------------------------------------

async fn open_connection(client: &reqwest::Client, app_token: &str) -> Result<String> {
    let resp = client
        .post("https://slack.com/api/apps.connections.open")
        .header("Authorization", format!("Bearer {app_token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await
        .context("Failed to call apps.connections.open")?;

    let body: Value = resp
        .json()
        .await
        .context("Failed to parse apps.connections.open response")?;

    if body["ok"].as_bool() != Some(true) {
        let err = body["error"].as_str().unwrap_or("unknown");
        anyhow::bail!("apps.connections.open failed: {err}");
    }

    body["url"]
        .as_str()
        .map(|s| s.to_string())
        .context("Missing 'url' in apps.connections.open response")
}

// ---------------------------------------------------------------------------
// WebSocket event loop
// ---------------------------------------------------------------------------

async fn run_ws_loop(state: &AppState, wss_url: &str) -> Result<String> {
    let (ws_stream, _response) = tokio_tungstenite::connect_async(wss_url)
        .await
        .context("Failed to connect WebSocket")?;

    let (mut write, mut read) = ws_stream.split();

    tracing::info!("WebSocket connected");

    while let Some(msg_result) = read.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                return Err(anyhow::anyhow!("WebSocket read error: {e}"));
            }
        };

        match msg {
            Message::Text(text) => {
                let envelope: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse WebSocket message");
                        continue;
                    }
                };

                // Immediately acknowledge the envelope
                if let Some(envelope_id) = envelope["envelope_id"].as_str() {
                    let ack = json!({"envelope_id": envelope_id}).to_string();
                    if let Err(e) = write.send(Message::Text(ack.into())).await {
                        tracing::warn!(error = %e, "Failed to send ack");
                    }
                }

                let envelope_type = envelope["type"].as_str().unwrap_or("");

                match envelope_type {
                    "hello" => {
                        tracing::info!("Socket Mode connection established (hello)");
                    }
                    "events_api" => {
                        let payload = envelope["payload"].clone();
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            relay::handle_slack_event(state_clone, payload).await;
                        });
                    }
                    "disconnect" => {
                        let reason = envelope["reason"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        tracing::info!(reason, "Received disconnect envelope");
                        return Ok(reason);
                    }
                    other => {
                        tracing::debug!(envelope_type = other, "Unhandled envelope type");
                    }
                }
            }
            Message::Ping(data) => {
                if let Err(e) = write.send(Message::Pong(data)).await {
                    tracing::warn!(error = %e, "Failed to send pong");
                }
            }
            Message::Close(_) => {
                return Ok("ws_close_frame".to_string());
            }
            _ => {}
        }
    }

    Ok("stream_ended".to_string())
}
