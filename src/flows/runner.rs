use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use tracing::Instrument;
use uuid::Uuid;

use tokio::sync::broadcast;

use crate::config::{SinkConfig, SourceConfig};
use crate::flows::events::{RunEvent, RunEventType};
use crate::flows::history::{FlowRun, NodeRun, RunStatus};
use crate::flows::store::Store;
use crate::flows::{Flow, NodeType};
use crate::github::client::GithubClient;
use crate::tasks::context::render_prompt;
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::Executor;
use crate::tasks::filters::Filter;
use crate::tasks::filters::keyword::{KeywordFilter, MatchField};
use crate::tasks::sources::{self, ContentItem};
use crate::tasks::pipeline::{format_items, resolve_sinks};

pub struct FlowRunner {
    pub http_client: Arc<reqwest::Client>,
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub events_tx: Option<broadcast::Sender<RunEvent>>,
}

impl FlowRunner {
    fn emit(
        &self,
        flow_id: &str,
        run_id: &str,
        node_id: Option<&str>,
        event_type: RunEventType,
        message: impl Into<String>,
    ) {
        if let Some(tx) = &self.events_tx {
            let _ = tx.send(RunEvent {
                flow_id: flow_id.to_string(),
                run_id: run_id.to_string(),
                timestamp: Utc::now(),
                node_id: node_id.map(String::from),
                event_type,
                message: message.into(),
            });
        }
    }
}

impl FlowRunner {
    pub async fn execute_with_context(
        &self,
        flow: &Flow,
        store: &dyn Store,
        extra_vars: HashMap<String, String>,
    ) -> Result<FlowRun> {
        let run_id = Uuid::new_v4().to_string();
        let short_id = &run_id[..8];
        let run = FlowRun {
            id: run_id.clone(),
            flow_id: flow.id.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            node_runs: vec![],
            error: None,
        };
        store.add_run(run.clone()).await?;
        self.emit(&flow.id, &run_id, None, RunEventType::RunStarted, "Flow execution started (with context)");

        let span = tracing::info_span!("flow_run", flow = %flow.name, run = %short_id);

        tracing::info!(parent: &span, nodes = flow.nodes.len(), edges = flow.edges.len(), "▶ Started (with context)");

        let start = std::time::Instant::now();
        let result = self.execute_inner_with_context(flow, &run_id, store, extra_vars).instrument(span.clone()).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                store
                    .complete_run(&flow.id, &run_id, RunStatus::Success, None)
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunCompleted, format!("Completed in {:.1}s", elapsed.as_secs_f64()));
                tracing::info!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), "✓ Completed");
            }
            Err(e) => {
                let err_msg = format!("{e:#}");
                store
                    .complete_run(&flow.id, &run_id, RunStatus::Failed, Some(err_msg.clone()))
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunFailed, &err_msg);
                tracing::error!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), error = %err_msg, "✗ Failed");
            }
        }

        let run = store
            .get_runs(&flow.id, 100)
            .await
            .into_iter()
            .find(|r| r.id == run_id)
            .unwrap_or(run);

        result.map(|_| run)
    }

    pub async fn execute(&self, flow: &Flow, store: &dyn Store) -> Result<FlowRun> {
        let run_id = Uuid::new_v4().to_string();
        let short_id = &run_id[..8];
        let run = FlowRun {
            id: run_id.clone(),
            flow_id: flow.id.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            node_runs: vec![],
            error: None,
        };
        store.add_run(run.clone()).await?;
        self.emit(&flow.id, &run_id, None, RunEventType::RunStarted, "Flow execution started");

        let span = tracing::info_span!("flow_run", flow = %flow.name, run = %short_id);

        tracing::info!(parent: &span, nodes = flow.nodes.len(), edges = flow.edges.len(), "▶ Started");

        let start = std::time::Instant::now();
        let result = self.execute_inner(flow, &run_id, store).instrument(span.clone()).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                store
                    .complete_run(&flow.id, &run_id, RunStatus::Success, None)
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunCompleted, format!("Completed in {:.1}s", elapsed.as_secs_f64()));
                tracing::info!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), "✓ Completed");
            }
            Err(e) => {
                let err_msg = format!("{e:#}");
                store
                    .complete_run(&flow.id, &run_id, RunStatus::Failed, Some(err_msg.clone()))
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunFailed, &err_msg);
                tracing::error!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), error = %err_msg, "✗ Failed");
            }
        }

        // Return the final state
        let run = store
            .get_runs(&flow.id, 100)
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
        store: &dyn Store,
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
                store.push_node_run(&flow.id, run_id, node_run).await?;
            }

            for node in &source_nodes {
                self.emit(&flow.id, run_id, Some(&node.id), RunEventType::NodeStarted, format!("Fetching {}...", node.label));
            }

            let fetch_start = std::time::Instant::now();
            let result =
                sources::fetch_all(&source_configs, &self.http_client, github_token.as_deref())
                    .await;
            let fetch_elapsed = fetch_start.elapsed();

            let source_msg = format!("{} items fetched in {:.1}s", result.len(), fetch_elapsed.as_secs_f64());
            for node in &source_nodes {
                self.emit(&flow.id, run_id, Some(&node.id), RunEventType::NodeCompleted, &source_msg);
            }

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
                let preview = format!("{} items fetched", result.len());
                store
                    .complete_node_run(&flow.id, run_id, &node.id, RunStatus::Success, Some(preview))
                    .await?;
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
            store.push_node_run(&flow.id, run_id, filter_run).await?;

            self.emit(&flow.id, run_id, Some(&node.id), RunEventType::NodeStarted, format!("Applying {}...", node.label));

            let filter = parse_filter_config(node)?;
            items = filter.apply(items);

            let dropped = before_count - items.len();
            let filter_msg = format!("{} → {} items ({} dropped)", before_count, items.len(), dropped);
            self.emit(&flow.id, run_id, Some(&node.id), RunEventType::NodeCompleted, &filter_msg);

            tracing::info!(
                filter = %node.label,
                "{} → {} items ({} dropped)",
                before_count,
                items.len(),
                dropped,
            );

            let preview = format!("{} → {} items ({} dropped)", before_count, items.len(), dropped);
            store
                .complete_node_run(&flow.id, run_id, &node.id, RunStatus::Success, Some(preview))
                .await?;
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

        self.emit(&flow.id, run_id, None, RunEventType::Log, format!("Prompt rendered ({} chars)", rendered.len()));

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
        store.push_node_run(&flow.id, run_id, exec_node_run).await?;

        self.emit(&flow.id, run_id, Some(&executor_node.id), RunEventType::NodeStarted, format!("Executing {}...", executor_node.kind));

        let exec_start = std::time::Instant::now();
        let exec_result = executor
            .execute(&rendered, &working_dir)
            .await
            .context("executor failed")?;
        let exec_elapsed = exec_start.elapsed();

        self.emit(
            &flow.id, run_id, Some(&executor_node.id), RunEventType::NodeCompleted,
            format!("{} turns, ${:.4}, {} chars output", exec_result.num_turns, exec_result.cost_usd, exec_result.text.len()),
        );

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

        store
            .complete_node_run(&flow.id, run_id, &executor_node.id, RunStatus::Success, Some(preview))
            .await?;

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
                store.push_node_run(&flow.id, run_id, sink_run).await?;

                self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeStarted, format!("Delivering to {}...", sink_node.label));

                let sink_start = std::time::Instant::now();
                let result = sink.deliver(&exec_result.text).await;
                let sink_elapsed = sink_start.elapsed();

                let (status, preview) = match &result {
                    Ok(_) => {
                        self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeCompleted, format!("Delivered in {:.1}s", sink_elapsed.as_secs_f64()));
                        tracing::info!(
                            sink = %sink_node.label,
                            elapsed = format_args!("{:.1}s", sink_elapsed.as_secs_f64()),
                            "✓ Delivered",
                        );
                        (RunStatus::Success, format!("Delivered in {:.1}s", sink_elapsed.as_secs_f64()))
                    }
                    Err(e) => {
                        self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeFailed, format!("Failed: {e}"));
                        tracing::error!(
                            sink = %sink_node.label,
                            error = %e,
                            "✗ Delivery failed",
                        );
                        (RunStatus::Failed, format!("Failed: {e}"))
                    }
                };

                store
                    .complete_node_run(&flow.id, run_id, &sink_node.id, status, Some(preview))
                    .await?;
            }
        } else {
            tracing::warn!("⚠ Executor returned empty output, skipping sinks");
        }

        Ok(())
    }

    /// Execute a flow with pre-built context variables (e.g. PR diff, metadata).
    /// Skips source fetching and filtering — the caller provides all template vars.
    async fn execute_inner_with_context(
        &self,
        flow: &Flow,
        run_id: &str,
        store: &dyn Store,
        extra_vars: HashMap<String, String>,
    ) -> Result<()> {
        let executor_node = flow
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Executor)
            .context("flow has no executor node")?;

        let trigger_node = flow.nodes.iter().find(|n| n.node_type == NodeType::Trigger);

        // Build template vars from extra_vars
        let mut vars = extra_vars;
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        vars.entry("timestamp".to_string()).or_insert(timestamp);

        // Resolve prompt
        let prompt_path = executor_node.config["prompt"]
            .as_str()
            .context("executor node missing 'prompt' config")?;

        let prompt_template = if prompt_path.ends_with(".md") || prompt_path.ends_with(".txt") || std::path::Path::new(prompt_path).exists() {
            std::fs::read_to_string(prompt_path)
                .with_context(|| format!("failed to read prompt file: {prompt_path}"))?
        } else {
            prompt_path.to_string()
        };

        let rendered = render_prompt(&prompt_template, &vars);

        self.emit(&flow.id, run_id, None, RunEventType::Log, format!("Prompt rendered ({} chars)", rendered.len()));

        tracing::info!(
            chars = rendered.len(),
            "✓ Prompt rendered (with context)",
        );

        // Resolve working_dir: trigger node config > executor node config > cwd
        let working_dir = trigger_node
            .and_then(|n| n.config["working_dir"].as_str())
            .or_else(|| executor_node.config["working_dir"].as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let permissions: Vec<String> = executor_node.config["permissions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let append_system_prompt = executor_node.config["append_system_prompt"]
            .as_str()
            .map(String::from);

        let executor = ClaudeCodeExecutor::new(permissions, append_system_prompt);

        let exec_node_run = NodeRun {
            node_id: executor_node.id.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            output_preview: None,
        };
        store.push_node_run(&flow.id, run_id, exec_node_run).await?;

        self.emit(&flow.id, run_id, Some(&executor_node.id), RunEventType::NodeStarted, format!("Executing {}...", executor_node.kind));

        let exec_start = std::time::Instant::now();
        let exec_result = executor
            .execute(&rendered, &working_dir)
            .await
            .context("executor failed")?;
        let exec_elapsed = exec_start.elapsed();

        self.emit(
            &flow.id, run_id, Some(&executor_node.id), RunEventType::NodeCompleted,
            format!("{} turns, ${:.4}, {} chars output", exec_result.num_turns, exec_result.cost_usd, exec_result.text.len()),
        );

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

        store
            .complete_node_run(&flow.id, run_id, &executor_node.id, RunStatus::Success, Some(preview))
            .await?;

        // Sinks
        let sink_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Sink)
            .collect();

        if !exec_result.text.is_empty() && !sink_nodes.is_empty() {
            let sink_configs = parse_sink_configs(&sink_nodes)?;
            let resolved_sinks = resolve_sinks(&sink_configs, &self.http_client)?;

            for (i, sink) in resolved_sinks.iter().enumerate() {
                let sink_node = &sink_nodes[i];
                self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeStarted, format!("Delivering to {}...", sink_node.label));
                let sink_start = std::time::Instant::now();
                let result = sink.deliver(&exec_result.text).await;
                let sink_elapsed = sink_start.elapsed();
                match &result {
                    Ok(_) => {
                        self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeCompleted, format!("Delivered in {:.1}s", sink_elapsed.as_secs_f64()));
                        tracing::info!(sink = %sink_node.label, "✓ Delivered");
                    }
                    Err(e) => {
                        self.emit(&flow.id, run_id, Some(&sink_node.id), RunEventType::NodeFailed, format!("Failed: {e}"));
                        tracing::error!(sink = %sink_node.label, error = %e, "✗ Delivery failed");
                    }
                }
            }
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
