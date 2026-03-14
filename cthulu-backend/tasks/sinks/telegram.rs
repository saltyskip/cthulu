use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;

use super::Sink;

pub struct TelegramSink {
    http_client: Arc<reqwest::Client>,
    bot_token: String,
    chat_id: String,
}

impl TelegramSink {
    pub fn new(http_client: Arc<reqwest::Client>, bot_token: String, chat_id: String) -> Self {
        Self { http_client, bot_token, chat_id }
    }
}

#[async_trait]
impl Sink for TelegramSink {
    async fn deliver(&self, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        // Telegram messages capped at 4096 chars. Truncate preserving char boundaries.
        let max_len = 4096;
        let msg = if text.len() > max_len {
            let mut end = max_len;
            while !text.is_char_boundary(end) { end -= 1; }
            &text[..end]
        } else {
            text
        };

        let body = json!({
            "chat_id": self.chat_id,
            "text": msg,
            "disable_web_page_preview": true,
        });

        let response = self.http_client.post(&url).json(&body).send().await
            .context("failed to call Telegram sendMessage")?;

        let status = response.status();
        let resp_body: serde_json::Value = response.json().await
            .context("failed to parse Telegram API response")?;

        if !status.is_success() || resp_body["ok"].as_bool() != Some(true) {
            let description = resp_body["description"].as_str().unwrap_or("unknown error");
            anyhow::bail!("Telegram sendMessage failed ({status}): {description}");
        }

        tracing::info!("Delivered message to Telegram");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_sink_construction() {
        let sink = TelegramSink::new(
            Arc::new(reqwest::Client::new()),
            "fake-token".to_string(),
            "123".to_string(),
        );
        assert_eq!(sink.chat_id, "123");
        assert_eq!(sink.bot_token, "fake-token");
    }
}
