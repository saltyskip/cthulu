use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::{SinkConfig, SourceConfig};
use crate::flows::history::{FlowRun, NodeRun, RunHistory, RunStatus};
use crate::flows::{Flow, NodeType};
use crate::github::client::GithubClient;
use crate::tasks::context::render_prompt;
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::Executor;
use crate::tasks::filters::Filter;
use crate::tasks::filters::keyword::{KeywordFilter, MatchField};
use crate::tasks::sources::{self, ContentItem};
use crate::tasks::triggers::cron::{format_items, resolve_sinks};

pub struct FlowRunner {
    pub http_client: Arc<reqwest::Client>,
    pub github_client: Option<Arc<dyn GithubClient>>,
}

impl FlowRunner {
    pub async fn execute(&self, flow: &Flow, history: &RunHistory) -> Result<FlowRun> {
        let run_id = Uuid::new_v4().to_string();
        let short_id = &run_id[..8];
        let mut run = FlowRun {
            id: run_id.clone(),
            flow_id: flow.id.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            node_runs: vec![],
            error: None,
        };
        history.add_run(run.clone()).await;

        let span = tracing::info_span!("flow_run", flow = %flow.name, run = %short_id);

        tracing::info!(parent: &span, nodes = flow.nodes.len(), edges = flow.edges.len(), "▶ Started");

        let start = std::time::Instant::now();
        let result = self.execute_inner(flow, &run_id, history).instrument(span.clone()).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                history
                    .update_run(&flow.id, &run_id, |r| {
                        r.status = RunStatus::Success;
                        r.finished_at = Some(Utc::now());
                    })
                    .await;
                tracing::info!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), "✓ Completed");
            }
            Err(e) => {
                let err_msg = format!("{e:#}");
                history
                    .update_run(&flow.id, &run_id, |r| {
                        r.status = RunStatus::Failed;
                        r.finished_at = Some(Utc::now());
                        r.error = Some(err_msg.clone());
                    })
                    .await;
                tracing::error!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), error = %err_msg, "✗ Failed");
            }
        }

        // Return the final state
        run = history
            .get_runs(&flow.id)
            .await
            .into_iter()
            .find(|r| r.id == run_id)
            .unwrap_or(run);

        result.map(|_| run)
    }

    async fn execute_inner(
        &self,
        flow: &Flow,
        run_id: &str,
        history: &RunHistory,
    ) -> Result<()> {
        // Find nodes by type
        let executor_node = flow
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Executor)
            .context("flow has no executor node")?;

        let source_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Source)
            .collect();

        let filter_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Filter)
            .collect();

        let sink_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Sink)
            .collect();

        tracing::info!(
            "Pipeline: {} source(s) → {} filter(s) → {} → {} sink(s)",
            source_nodes.len(),
            filter_nodes.len(),
            executor_node.kind,
            sink_nodes.len(),
        );

        // ── 1. SOURCES ──────────────────────────────────────────────
        let source_configs = parse_source_configs(&source_nodes)?;
        let github_token = self
            .github_client
            .as_ref()
            .map(|_| std::env::var("GITHUB_TOKEN").ok())
            .flatten();

        let items: Vec<ContentItem> = if !source_configs.is_empty() {
            // Record node runs for tracking
            for node in &source_nodes {
                let node_run = NodeRun {
                    node_id: node.id.clone(),
                    status: RunStatus::Running,
                    started_at: Utc::now(),
                    finished_at: None,
                    output_preview: None,
                };
                history
                    .update_run(&flow.id, run_id, |r| r.node_runs.push(node_run))
                    .await;
            }

            let fetch_start = std::time::Instant::now();
            let result =
                sources::fetch_all(&source_configs, &self.http_client, github_token.as_deref())
                    .await;
            let fetch_elapsed = fetch_start.elapsed();

            tracing::info!(
                items = result.len(),
                elapsed = format_args!("{:.1}s", fetch_elapsed.as_secs_f64()),
                "✓ Sources fetched",
            );

            // Log a sample of item titles at debug
            for (i, item) in result.iter().take(5).enumerate() {
                tracing::debug!(
                    "[{}/{}] {} ({})",
                    i + 1,
                    result.len(),
                    truncate(&item.title, 80),
                    truncate(&item.url, 60),
                );
            }
            if result.len() > 5 {
                tracing::debug!("... and {} more items", result.len() - 5);
            }

            // Mark source nodes as complete
            for node in &source_nodes {
                let nid = node.id.clone();
                let preview = format!("{} items fetched", result.len());
                history
                    .update_run(&flow.id, run_id, |r| {
                        if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == nid) {
                            nr.status = RunStatus::Success;
                            nr.finished_at = Some(Utc::now());
                            nr.output_preview = Some(preview);
                        }
                    })
                    .await;
            }

            result
        } else {
            tracing::info!("No sources configured, skipping fetch");
            vec![]
        };

        // ── 2. FILTERS ──────────────────────────────────────────────
        let mut items = items;
        for node in &filter_nodes {
            let before_count = items.len();

            let filter_run = NodeRun {
                node_id: node.id.clone(),
                status: RunStatus::Running,
                started_at: Utc::now(),
                finished_at: None,
                output_preview: None,
            };
            history
                .update_run(&flow.id, run_id, |r| r.node_runs.push(filter_run))
                .await;

            let filter = parse_filter_config(node)?;
            items = filter.apply(items);

            let dropped = before_count - items.len();
            tracing::info!(
                filter = %node.label,
                "{} → {} items ({} dropped)",
                before_count,
                items.len(),
                dropped,
            );

            let nid = node.id.clone();
            let preview = format!("{} → {} items ({} dropped)", before_count, items.len(), dropped);
            history
                .update_run(&flow.id, run_id, |r| {
                    if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == nid) {
                        nr.status = RunStatus::Success;
                        nr.finished_at = Some(Utc::now());
                        nr.output_preview = Some(preview);
                    }
                })
                .await;
        }

        // ── 3. PROMPT RENDERING ─────────────────────────────────────
        let content = format_items(&items);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content);
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);

        let prompt_path = executor_node.config["prompt"]
            .as_str()
            .context("executor node missing 'prompt' config")?;

        let prompt_template = if prompt_path.ends_with(".md") || prompt_path.ends_with(".txt") || std::path::Path::new(prompt_path).exists() {
            tracing::info!(path = %prompt_path, "Loading prompt from file");
            std::fs::read_to_string(prompt_path)
                .with_context(|| format!("failed to read prompt file: {prompt_path}"))?
        } else {
            tracing::info!(len = prompt_path.len(), "Using inline prompt");
            prompt_path.to_string()
        };

        // Fetch market data if needed
        if prompt_template.contains("{{market_data}}") {
            tracing::info!("Fetching market data");
            let market_data = match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                crate::tasks::sources::market::fetch_market_snapshot(&self.http_client),
            )
            .await
            {
                Ok(Ok(data)) => {
                    tracing::info!(len = data.len(), "✓ Market data fetched");
                    data
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "⚠ Market data fetch failed, using fallback");
                    "Market data unavailable.".to_string()
                }
                Err(_) => {
                    tracing::warn!("⚠ Market data fetch timed out, using fallback");
                    "Market data unavailable.".to_string()
                }
            };
            vars.insert("market_data".to_string(), market_data);
        }

        let rendered = render_prompt(&prompt_template, &vars);

        // If the prompt didn't contain {{content}} but we have source data, append it
        let rendered = if !items.is_empty() && !prompt_template.contains("{{content}}") {
            tracing::debug!("Prompt has no {{content}} placeholder, appending source data");
            format!("{rendered}\n\n<<<\n{}\n>>>", vars.get("content").cloned().unwrap_or_default())
        } else {
            rendered
        };

        tracing::info!(
            chars = rendered.len(),
            items = items.len(),
            "✓ Prompt rendered",
        );

        // ── 4. EXECUTOR ─────────────────────────────────────────────
        let permissions: Vec<String> = executor_node.config["permissions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let working_dir = executor_node.config["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let append_system_prompt = executor_node.config["append_system_prompt"]
            .as_str()
            .map(String::from);

        let has_system_prompt = append_system_prompt.is_some();
        let executor = ClaudeCodeExecutor::new(permissions.clone(), append_system_prompt);

        let perms_display = if permissions.is_empty() { "ALL".to_string() } else { permissions.join(", ") };
        tracing::info!(
            executor = %executor_node.kind,
            permissions = %perms_display,
            system_prompt = has_system_prompt,
            "⟶ Executing",
        );

        let exec_node_run = NodeRun {
            node_id: executor_node.id.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            output_preview: None,
        };
        history
            .update_run(&flow.id, run_id, |r| r.node_runs.push(exec_node_run))
            .await;

        let exec_start = std::time::Instant::now();
        let exec_result = executor
            .execute(&rendered, &working_dir)
            .await
            .context("executor failed")?;
        let exec_elapsed = exec_start.elapsed();

        tracing::info!(
            turns = exec_result.num_turns,
            cost = format_args!("${:.4}", exec_result.cost_usd),
            output_chars = exec_result.text.len(),
            elapsed = format_args!("{:.1}s", exec_elapsed.as_secs_f64()),
            "✓ Executor finished",
        );

        let preview = if exec_result.text.len() > 500 {
            format!("{}...", &exec_result.text[..500])
        } else {
            exec_result.text.clone()
        };

        let exec_nid = executor_node.id.clone();
        history
            .update_run(&flow.id, run_id, |r| {
                if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == exec_nid) {
                    nr.status = RunStatus::Success;
                    nr.finished_at = Some(Utc::now());
                    nr.output_preview = Some(preview);
                }
            })
            .await;

        // ── 5. SINKS ────────────────────────────────────────────────
        if !exec_result.text.is_empty() {
            let sink_configs = parse_sink_configs(&sink_nodes)?;
            let resolved_sinks = resolve_sinks(&sink_configs, &self.http_client)?;

            for (i, sink) in resolved_sinks.iter().enumerate() {
                let sink_node = &sink_nodes[i];

                let sink_run = NodeRun {
                    node_id: sink_node.id.clone(),
                    status: RunStatus::Running,
                    started_at: Utc::now(),
                    finished_at: None,
                    output_preview: None,
                };
                history
                    .update_run(&flow.id, run_id, |r| r.node_runs.push(sink_run))
                    .await;

                let sink_start = std::time::Instant::now();
                let result = sink.deliver(&exec_result.text).await;
                let sink_elapsed = sink_start.elapsed();

                let nid = sink_node.id.clone();
                let (status, preview) = match &result {
                    Ok(_) => {
                        tracing::info!(
                            sink = %sink_node.label,
                            elapsed = format_args!("{:.1}s", sink_elapsed.as_secs_f64()),
                            "✓ Delivered",
                        );
                        (RunStatus::Success, format!("Delivered in {:.1}s", sink_elapsed.as_secs_f64()))
                    }
                    Err(e) => {
                        tracing::error!(
                            sink = %sink_node.label,
                            error = %e,
                            "✗ Delivery failed",
                        );
                        (RunStatus::Failed, format!("Failed: {e}"))
                    }
                };

                history
                    .update_run(&flow.id, run_id, |r| {
                        if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == nid) {
                            nr.status = status;
                            nr.finished_at = Some(Utc::now());
                            nr.output_preview = Some(preview);
                        }
                    })
                    .await;
            }
        } else {
            tracing::warn!("⚠ Executor returned empty output, skipping sinks");
        }

        Ok(())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

fn parse_source_configs(
    nodes: &[&crate::flows::Node],
) -> Result<Vec<SourceConfig>> {
    let mut configs = Vec::new();
    for node in nodes {
        let config = match node.kind.as_str() {
            "rss" => {
                let url = node.config["url"]
                    .as_str()
                    .context("rss node missing 'url'")?
                    .to_string();
                let limit = node.config["limit"].as_u64().unwrap_or(10) as usize;
                let keywords = node.config["keywords"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                SourceConfig::Rss { url, limit, keywords }
            }
            "web-scrape" => {
                let url = node.config["url"]
                    .as_str()
                    .context("web-scrape node missing 'url'")?
                    .to_string();
                let keywords = node.config["keywords"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                SourceConfig::WebScrape { url, keywords }
            }
            "github-merged-prs" => {
                let repos = node.config["repos"]
                    .as_array()
                    .context("github-merged-prs node missing 'repos'")?
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                let since_days = node.config["since_days"].as_u64().unwrap_or(7);
                SourceConfig::GithubMergedPrs { repos, since_days }
            }
            "web-scraper" => {
                let url = node.config["url"]
                    .as_str()
                    .context("web-scraper node missing 'url'")?
                    .to_string();
                let base_url = node.config["base_url"].as_str().map(String::from);
                let items_selector = node.config["items_selector"]
                    .as_str()
                    .context("web-scraper node missing 'items_selector'")?
                    .to_string();
                let title_selector = node.config["title_selector"].as_str().map(String::from);
                let url_selector = node.config["url_selector"].as_str().map(String::from);
                let summary_selector = node.config["summary_selector"].as_str().map(String::from);
                let date_selector = node.config["date_selector"].as_str().map(String::from);
                let date_format = node.config["date_format"].as_str().map(String::from);
                let limit = node.config["limit"].as_u64().unwrap_or(10) as usize;
                SourceConfig::WebScraper {
                    url, base_url, items_selector, title_selector,
                    url_selector, summary_selector, date_selector,
                    date_format, limit,
                }
            }
            "market-data" => {
                // Market data is handled specially via template variable
                continue;
            }
            other => bail!("unknown source kind: {other}"),
        };
        configs.push(config);
    }
    Ok(configs)
}

fn parse_filter_config(node: &crate::flows::Node) -> Result<Box<dyn Filter>> {
    match node.kind.as_str() {
        "keyword" => {
            let keywords: Vec<String> = node.config["keywords"]
                .as_array()
                .context("keyword filter missing 'keywords'")?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            let require_all = node.config["require_all"].as_bool().unwrap_or(false);
            let field = match node.config["field"].as_str().unwrap_or("title_or_summary") {
                "title" => MatchField::Title,
                "summary" => MatchField::Summary,
                _ => MatchField::TitleOrSummary,
            };
            Ok(Box::new(KeywordFilter::new(keywords, require_all, field)))
        }
        other => bail!("unknown filter kind: {other}"),
    }
}

fn parse_sink_configs(nodes: &[&crate::flows::Node]) -> Result<Vec<SinkConfig>> {
    let mut configs = Vec::new();
    for node in nodes {
        let config = match node.kind.as_str() {
            "slack" => SinkConfig::Slack {
                webhook_url_env: node.config["webhook_url_env"]
                    .as_str()
                    .map(String::from),
                bot_token_env: node.config["bot_token_env"].as_str().map(String::from),
                channel: node.config["channel"].as_str().map(String::from),
            },
            "notion" => SinkConfig::Notion {
                token_env: node.config["token_env"]
                    .as_str()
                    .context("notion node missing 'token_env'")?
                    .to_string(),
                database_id: node.config["database_id"]
                    .as_str()
                    .context("notion node missing 'database_id'")?
                    .to_string(),
            },
            other => bail!("unknown sink kind: {other}"),
        };
        configs.push(config);
    }
    Ok(configs)
}
