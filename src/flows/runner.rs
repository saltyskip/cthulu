use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use uuid::Uuid;

use crate::config::{SinkConfig, SourceConfig};
use crate::flows::history::{FlowRun, NodeRun, RunHistory, RunStatus};
use crate::flows::{Flow, NodeType};
use crate::github::client::GithubClient;
use crate::tasks::context::render_prompt;
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::Executor;
use crate::tasks::sources::{self, ContentItem};
use crate::tasks::triggers::cron::{format_items, resolve_sinks};

pub struct FlowRunner {
    pub http_client: Arc<reqwest::Client>,
    pub github_client: Option<Arc<dyn GithubClient>>,
}

impl FlowRunner {
    pub async fn execute(&self, flow: &Flow, history: &RunHistory) -> Result<FlowRun> {
        let run_id = Uuid::new_v4().to_string();
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

        let result = self.execute_inner(flow, &run_id, history).await;

        match &result {
            Ok(_) => {
                history
                    .update_run(&flow.id, &run_id, |r| {
                        r.status = RunStatus::Success;
                        r.finished_at = Some(Utc::now());
                    })
                    .await;
            }
            Err(e) => {
                let err_msg = format!("{e:#}");
                history
                    .update_run(&flow.id, &run_id, |r| {
                        r.status = RunStatus::Failed;
                        r.finished_at = Some(Utc::now());
                        r.error = Some(err_msg);
                    })
                    .await;
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

        let sink_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Sink)
            .collect();

        // 1. Fetch sources concurrently
        let source_configs = parse_source_configs(&source_nodes)?;
        let github_token = self
            .github_client
            .as_ref()
            .map(|_| std::env::var("GITHUB_TOKEN").ok())
            .flatten();

        let items: Vec<ContentItem> = if !source_configs.is_empty() {
            // Record source node runs
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

            let result =
                sources::fetch_all(&source_configs, &self.http_client, github_token.as_deref())
                    .await;

            // Mark source nodes as complete
            for node in &source_nodes {
                let nid = node.id.clone();
                history
                    .update_run(&flow.id, run_id, |r| {
                        if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == nid) {
                            nr.status = RunStatus::Success;
                            nr.finished_at = Some(Utc::now());
                        }
                    })
                    .await;
            }

            result
        } else {
            vec![]
        };

        // 2. Build template variables
        let content = format_items(&items);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content);
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);

        // 3. Read and render prompt template
        let prompt_path = executor_node.config["prompt"]
            .as_str()
            .context("executor node missing 'prompt' config")?;

        let prompt_template = std::fs::read_to_string(prompt_path)
            .with_context(|| format!("failed to read prompt file: {prompt_path}"))?;

        // Fetch market data if needed
        if prompt_template.contains("{{market_data}}") {
            let market_data = match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                crate::tasks::sources::market::fetch_market_snapshot(&self.http_client),
            )
            .await
            {
                Ok(Ok(data)) => data,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "Failed to fetch market data");
                    "Market data unavailable.".to_string()
                }
                Err(_) => {
                    tracing::warn!("Market data fetch timed out");
                    "Market data unavailable.".to_string()
                }
            };
            vars.insert("market_data".to_string(), market_data);
        }

        let rendered = render_prompt(&prompt_template, &vars);

        // 4. Execute
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

        let executor = ClaudeCodeExecutor::new(permissions);

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

        let exec_result = executor
            .execute(&rendered, &working_dir)
            .await
            .context("executor failed")?;

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

        // 5. Deliver to sinks
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

                let result = sink.deliver(&exec_result.text).await;
                let nid = sink_node.id.clone();
                let status = if result.is_ok() {
                    RunStatus::Success
                } else {
                    RunStatus::Failed
                };
                history
                    .update_run(&flow.id, run_id, |r| {
                        if let Some(nr) = r.node_runs.iter_mut().find(|nr| nr.node_id == nid) {
                            nr.status = status;
                            nr.finished_at = Some(Utc::now());
                        }
                    })
                    .await;

                if let Err(e) = result {
                    tracing::error!(error = %e, "Failed to deliver to sink");
                }
            }
        }

        Ok(())
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
                SourceConfig::Rss { url, limit }
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
