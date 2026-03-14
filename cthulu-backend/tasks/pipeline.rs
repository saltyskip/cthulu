use std::sync::Arc;

use anyhow::{Context, Result};

use crate::config::SinkConfig;
use crate::tasks::sinks::notion::NotionSink;
use crate::tasks::sinks::slack::{SlackApiSink, SlackWebhookSink};
use crate::tasks::sinks::telegram::TelegramSink;
use crate::tasks::sinks::Sink;
use crate::tasks::sources::ContentItem;

/// Read a secret value. Strategy:
/// 1. If `env_name` is provided, try `std::env::var(env_name)`.
/// 2. If that fails (or env_name is None), try reading `key` from `~/.cthulu/secrets.json`.
/// 3. If both fail, return Err.
fn read_secret(env_name: Option<&str>, secrets_key: &str) -> Result<String> {
    if let Some(name) = env_name {
        if let Ok(val) = std::env::var(name) {
            if !val.is_empty() {
                return Ok(val);
            }
        }
    }

    let base_dir = dirs::home_dir()
        .map(|h| h.join(".cthulu"))
        .unwrap_or_else(|| std::path::PathBuf::from(".cthulu"));
    let secrets_path = base_dir.join("secrets.json");

    if secrets_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&secrets_path) {
            if let Ok(secrets) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(val) = secrets[secrets_key].as_str() {
                    if !val.is_empty() {
                        return Ok(val.to_string());
                    }
                }
            }
        }
    }

    let source = env_name
        .map(|n| format!("env var '{n}' or secrets.json key '{secrets_key}'"))
        .unwrap_or_else(|| format!("secrets.json key '{secrets_key}'"));
    anyhow::bail!("secret not found: {source}")
}

pub fn resolve_sinks(
    configs: &[SinkConfig],
    http_client: &Arc<reqwest::Client>,
) -> Result<Vec<Arc<dyn Sink>>> {
    let mut sinks: Vec<Arc<dyn Sink>> = Vec::with_capacity(configs.len());

    for config in configs {
        match config {
            SinkConfig::Slack {
                webhook_url_env,
                bot_token_env,
                channel,
            } => {
                if let Some(token_env) = bot_token_env {
                    let bot_token =
                        read_secret(Some(token_env), "slack_bot_token").with_context(|| {
                            format!(
                                "slack bot sink requires token (env: {token_env} or secrets.json)"
                            )
                        })?;
                    let channel = channel
                        .as_ref()
                        .with_context(|| "slack bot_token_env requires a channel to be set")?;
                    sinks.push(Arc::new(SlackApiSink::new(
                        Arc::clone(http_client),
                        bot_token,
                        channel.clone(),
                    )));
                } else if let Some(webhook_env) = webhook_url_env {
                    let webhook_url = read_secret(Some(webhook_env), "slack_webhook_url")
                        .with_context(|| format!("slack webhook sink requires URL (env: {webhook_env} or secrets.json)"))?;
                    sinks.push(Arc::new(SlackWebhookSink::new(
                        Arc::clone(http_client),
                        webhook_url,
                    )));
                } else {
                    let webhook_url = read_secret(None, "slack_webhook_url")
                        .context("slack sink requires either webhook_url_env, bot_token_env, or slack_webhook_url in secrets.json")?;
                    sinks.push(Arc::new(SlackWebhookSink::new(
                        Arc::clone(http_client),
                        webhook_url,
                    )));
                }
            }
            SinkConfig::Notion {
                token_env,
                database_id,
            } => {
                let token = read_secret(Some(token_env), "notion_token").with_context(|| {
                    format!("notion sink requires token (env: {token_env} or secrets.json)")
                })?;
                // Use config database_id if non-empty, otherwise fall back to secrets.json
                let db_id = if !database_id.is_empty() {
                    database_id.clone()
                } else {
                    read_secret(None, "notion_database_id")
                        .context("notion sink requires database_id in config or notion_database_id in secrets.json")?
                };
                sinks.push(Arc::new(NotionSink::new(
                    Arc::clone(http_client),
                    token,
                    db_id,
                )));
            }
            SinkConfig::Telegram {
                bot_token_env,
                chat_id,
            } => {
                let bot_token = read_secret(bot_token_env.as_deref(), "telegram_bot_token")
                    .context("telegram sink requires bot token (env var or secrets.json)")?;
                sinks.push(Arc::new(TelegramSink::new(
                    Arc::clone(http_client),
                    bot_token,
                    chat_id.clone(),
                )));
            }
        }
    }

    Ok(sinks)
}

pub fn format_items(items: &[ContentItem]) -> String {
    if items.is_empty() {
        return "No items fetched.".to_string();
    }

    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let published = item
                .published
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "unknown date".to_string());

            let summary_short = if item.summary.len() > 500 {
                let mut end = 500;
                while !item.summary.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &item.summary[..end])
            } else {
                item.summary.clone()
            };

            let image_line = item
                .image_url
                .as_deref()
                .map(|u| format!("\n   Image: {u}"))
                .unwrap_or_default();

            format!(
                "{}. [{}]({})\n   Published: {}{}\n   {}\n",
                i + 1,
                item.title,
                item.url,
                published,
                image_line,
                summary_short
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_items_empty() {
        let result = format_items(&[]);
        assert_eq!(result, "No items fetched.");
    }

    #[test]
    fn test_format_items_with_content() {
        let items = vec![
            ContentItem {
                title: "Bitcoin Hits ATH".to_string(),
                url: "https://example.com/1".to_string(),
                summary: "Bitcoin reached a new all-time high.".to_string(),
                published: None,
                image_url: None,
            },
            ContentItem {
                title: "ETH Update".to_string(),
                url: "https://example.com/2".to_string(),
                summary: "Ethereum ships a major update.".to_string(),
                published: None,
                image_url: Some("https://example.com/eth.jpg".to_string()),
            },
        ];
        let result = format_items(&items);
        assert!(result.contains("1. [Bitcoin Hits ATH](https://example.com/1)"));
        assert!(result.contains("2. [ETH Update](https://example.com/2)"));
        assert!(result.contains("https://example.com/1"));
        assert!(!result.contains("Image: https://example.com/1"));
        assert!(result.contains("Image: https://example.com/eth.jpg"));
    }
}
