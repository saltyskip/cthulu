pub mod blocks;
pub mod markdown;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;

use blocks::*;
use markdown::markdown_to_blocks;

use super::Sink;

// ---------------------------------------------------------------------------
// SlackWebhookSink
// ---------------------------------------------------------------------------

pub struct SlackWebhookSink {
    http_client: Arc<reqwest::Client>,
    webhook_url: String,
}

impl SlackWebhookSink {
    pub fn new(http_client: Arc<reqwest::Client>, webhook_url: String) -> Self {
        Self { http_client, webhook_url }
    }
}

#[async_trait]
impl Sink for SlackWebhookSink {
    async fn deliver(&self, text: &str) -> Result<()> {
        post_to_url(&self.http_client, &self.webhook_url, text).await
    }
}

// ---------------------------------------------------------------------------
// SlackApiSink
// ---------------------------------------------------------------------------

pub struct SlackApiSink {
    http_client: Arc<reqwest::Client>,
    bot_token: String,
    channel: String,
}

impl SlackApiSink {
    pub fn new(http_client: Arc<reqwest::Client>, bot_token: String, channel: String) -> Self {
        Self { http_client, bot_token, channel }
    }
}

#[async_trait]
impl Sink for SlackApiSink {
    async fn deliver(&self, text: &str) -> Result<()> {
        post_threaded_blocks(&self.http_client, &self.bot_token, &self.channel, text).await
    }
}

// ---------------------------------------------------------------------------
// Webhook (legacy) path
// ---------------------------------------------------------------------------

async fn post_to_url(
    client: &reqwest::Client,
    webhook_url: &str,
    text: &str,
) -> Result<()> {
    let slack_text = markdown::markdown_to_slack(text);

    let response = client
        .post(webhook_url)
        .json(&json!({ "text": slack_text }))
        .send()
        .await
        .context("failed to post to Slack webhook")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Slack webhook returned {status}: {body}");
    }

    tracing::info!("Delivered message to Slack");
    Ok(())
}

// ---------------------------------------------------------------------------
// Web API (Block Kit + threading) path
// ---------------------------------------------------------------------------

/// Post a message with Block Kit formatting and optional threading.
///
/// If `full_text` contains a `---THREAD---` delimiter, the part above becomes
/// the main channel message and the part below is posted as a thread reply.
async fn post_threaded_blocks(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    full_text: &str,
) -> Result<()> {
    let parts: Vec<&str> = full_text.splitn(2, "---THREAD---").collect();

    let main_text = parts[0].trim();
    let thread_text = parts.get(1).map(|s| s.trim());

    let main_blocks = markdown_to_blocks(main_text);
    let ts = post_blocks(client, bot_token, channel, &main_blocks, None)
        .await
        .context("failed to post main message")?;

    if let Some(detail) = thread_text {
        if !detail.is_empty() {
            let thread_blocks = markdown_to_blocks(detail);
            post_blocks(client, bot_token, channel, &thread_blocks, Some(&ts))
                .await
                .context("failed to post thread reply")?;
        }
    }

    tracing::info!("Delivered Block Kit message to Slack");
    Ok(())
}

/// Post blocks to Slack via `chat.postMessage`. Returns the message `ts`.
async fn post_blocks(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    blocks: &[Block],
    thread_ts: Option<&str>,
) -> Result<String> {
    let blocks = if blocks.len() > MAX_BLOCKS_PER_MESSAGE {
        let mut truncated = blocks[..MAX_BLOCKS_PER_MESSAGE - 1].to_vec();
        truncated.push(Block::Section {
            text: TextObject {
                kind: "mrkdwn",
                text: "_Message truncated — too many blocks._".to_string(),
            },
        });
        truncated
    } else {
        blocks.to_vec()
    };

    // Build a fallback plain-text summary from all text-bearing blocks
    let fallback: String = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Header { text } | Block::Section { text } => Some(text.text.clone()),
            Block::SectionFields { fields } => {
                let parts: Vec<&str> = fields.iter().map(|f| f.text.as_str()).collect();
                Some(parts.join(" | "))
            }
            Block::Context { elements } => {
                let parts: Vec<&str> = elements
                    .iter()
                    .map(|e| match e {
                        ContextElement::Mrkdwn { text } => text.as_str(),
                    })
                    .collect();
                Some(parts.join(" "))
            }
            Block::RichText { elements } => {
                let mut parts = Vec::new();
                for el in elements {
                    match el {
                        RichTextElement::Section { elements: inlines } => {
                            parts.push(extract_inline_text(inlines));
                        }
                        RichTextElement::List { elements: items, .. } => {
                            for item in items {
                                parts.push(format!("• {}", extract_inline_text(&item.elements)));
                            }
                        }
                    }
                }
                Some(parts.join("\n"))
            }
            Block::Divider => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut body = json!({
        "channel": channel,
        "blocks": blocks,
        "text": fallback,
    });

    if let Some(ts) = thread_ts {
        body["thread_ts"] = json!(ts);
    }

    let response = client
        .post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {bot_token}"))
        .json(&body)
        .send()
        .await
        .context("failed to call chat.postMessage")?;

    let status = response.status();
    let resp_body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse Slack API response")?;

    if !status.is_success() || resp_body["ok"].as_bool() != Some(true) {
        let err = resp_body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("chat.postMessage failed ({status}): {err}");
    }

    resp_body["ts"]
        .as_str()
        .map(|s| s.to_string())
        .context("Slack response missing ts field")
}

/// Extract plain text from a slice of rich text inlines.
fn extract_inline_text(inlines: &[RichTextInline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            RichTextInline::Text { text, .. } => text.clone(),
            RichTextInline::Link { text, url, .. } => {
                text.clone().unwrap_or_else(|| url.clone())
            }
            RichTextInline::Emoji { name } => format!(":{name}:"),
        })
        .collect::<Vec<_>>()
        .join("")
}
