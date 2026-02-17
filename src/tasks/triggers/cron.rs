use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use croner::Cron;

use crate::config::{SinkConfig, SourceConfig};
use crate::tasks::context::render_prompt;
use crate::tasks::executors::Executor;
use crate::tasks::sinks::Sink;
use crate::tasks::sinks::notion::NotionSink;
use crate::tasks::sinks::slack::{SlackApiSink, SlackWebhookSink};
use crate::tasks::sources::{self, ContentItem};

pub struct CronTrigger {
    cron: Cron,
    sources: Vec<SourceConfig>,
    sinks: Vec<Arc<dyn Sink>>,
    working_dir: PathBuf,
    http_client: Arc<reqwest::Client>,
    github_token: Option<String>,
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

impl CronTrigger {
    pub fn new(
        schedule: &str,
        sources: Vec<SourceConfig>,
        sinks: Vec<SinkConfig>,
        working_dir: PathBuf,
        http_client: Arc<reqwest::Client>,
        github_token: Option<String>,
    ) -> Result<Self> {
        let cron = Cron::new(schedule)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid cron expression '{}': {}", schedule, e))?;

        let resolved_sinks = resolve_sinks(&sinks, &http_client)?;

        Ok(Self {
            cron,
            sources,
            sinks: resolved_sinks,
            working_dir,
            http_client,
            github_token,
        })
    }

    pub async fn run_loop(
        &self,
        task_name: &str,
        prompt_template: &str,
        executor: &dyn Executor,
    ) {
        tracing::info!(task = %task_name, "Cron trigger started");

        loop {
            let now = Utc::now();
            let next = match self.cron.find_next_occurrence(&now, false) {
                Ok(next) => next,
                Err(e) => {
                    tracing::error!(task = %task_name, error = %e, "Failed to compute next cron occurrence");
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            };

            let duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(1));
            tracing::info!(
                task = %task_name,
                next = %next.format("%Y-%m-%d %H:%M:%S UTC"),
                "Sleeping until next cron fire"
            );
            tokio::time::sleep(duration).await;

            // Guard against premature wake from sleep imprecision
            let now_after = Utc::now();
            if now_after < next {
                let remaining = (next - now_after).to_std().unwrap_or_default();
                tokio::time::sleep(remaining).await;
            }

            if let Err(e) = self.execute_once(task_name, prompt_template, executor).await {
                tracing::error!(task = %task_name, error = %e, "Cron task execution failed");
            }
        }
    }

    pub(crate) async fn execute_once(
        &self,
        task_name: &str,
        prompt_template: &str,
        executor: &dyn Executor,
    ) -> Result<()> {
        tracing::info!(task = %task_name, "Cron task firing");

        // 1. Fetch content from sources
        let items = sources::fetch_all(&self.sources, &self.http_client, self.github_token.as_deref()).await;
        tracing::info!(task = %task_name, items = items.len(), "Fetched content items");

        // 2. Format items and build template variables
        let content = format_items(&items);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content);
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);

        // 3. Render prompt
        let rendered = render_prompt(prompt_template, &vars);

        // 4. Execute via Claude Code
        let exec_result = executor
            .execute(&rendered, &self.working_dir)
            .await
            .context("executor failed")?;

        tracing::info!(
            task = %task_name,
            cost_usd = exec_result.cost_usd,
            turns = exec_result.num_turns,
            "Cron task completed ({} turns, ${:.4})",
            exec_result.num_turns,
            exec_result.cost_usd
        );

        // 5. Deliver to all sinks (best-effort — one failure doesn't block others)
        if !exec_result.text.is_empty() {
            for sink in &self.sinks {
                if let Err(e) = sink.deliver(&exec_result.text).await {
                    tracing::error!(task = %task_name, error = %e, "Failed to deliver to sink");
                }
            }
        } else if !self.sinks.is_empty() {
            tracing::warn!(task = %task_name, "Executor returned empty text, skipping sink delivery");
        }

        Ok(())
    }
}

fn format_items(items: &[ContentItem]) -> String {
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

            format!(
                "{}. **{}**\n   URL: {}\n   Published: {}\n   {}\n",
                i + 1,
                item.title,
                item.url,
                published,
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
    fn test_cron_parse_standard_5_field() {
        let trigger = CronTrigger::new(
            "0 */4 * * *",
            vec![],
            vec![],
            PathBuf::from("/tmp"),
            Arc::new(reqwest::Client::new()),
            None,
        );
        assert!(trigger.is_ok());
    }

    #[test]
    fn test_cron_parse_invalid() {
        let trigger = CronTrigger::new(
            "not a cron",
            vec![],
            vec![],
            PathBuf::from("/tmp"),
            Arc::new(reqwest::Client::new()),
            None,
        );
        assert!(trigger.is_err());
    }

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
            },
            ContentItem {
                title: "ETH Update".to_string(),
                url: "https://example.com/2".to_string(),
                summary: "Ethereum ships a major update.".to_string(),
                published: None,
            },
        ];
        let result = format_items(&items);
        assert!(result.contains("1. **Bitcoin Hits ATH**"));
        assert!(result.contains("2. **ETH Update**"));
        assert!(result.contains("https://example.com/1"));
    }
}
