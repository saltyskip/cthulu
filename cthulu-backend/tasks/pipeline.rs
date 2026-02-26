use std::sync::Arc;

use anyhow::{Context, Result};

use crate::config::SinkConfig;
use crate::tasks::sinks::Sink;
use crate::tasks::sinks::notion::NotionSink;
use crate::tasks::sinks::slack::{SlackApiSink, SlackWebhookSink};
use crate::tasks::sources::ContentItem;

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
                    let bot_token = std::env::var(token_env).with_context(|| {
                        format!("sink requires env var {token_env} but it is not set")
                    })?;
                    let channel = channel.as_ref().with_context(|| {
                        "slack bot_token_env requires a channel to be set"
                    })?;
                    sinks.push(Arc::new(SlackApiSink::new(
                        Arc::clone(http_client),
                        bot_token,
                        channel.clone(),
                    )));
                } else if let Some(webhook_env) = webhook_url_env {
                    let webhook_url = std::env::var(webhook_env).with_context(|| {
                        format!("sink requires env var {webhook_env} but it is not set")
                    })?;
                    sinks.push(Arc::new(SlackWebhookSink::new(
                        Arc::clone(http_client),
                        webhook_url,
                    )));
                } else {
                    anyhow::bail!("slack sink requires either webhook_url_env or bot_token_env");
                }
            }
            SinkConfig::Notion {
                token_env,
                database_id,
            } => {
                let token = std::env::var(token_env).with_context(|| {
                    format!("sink requires env var {token_env} but it is not set")
                })?;
                sinks.push(Arc::new(NotionSink::new(
                    Arc::clone(http_client),
                    token,
                    database_id.clone(),
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
