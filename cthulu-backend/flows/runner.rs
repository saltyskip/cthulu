use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use tracing::Instrument;
use uuid::Uuid;

use tokio::sync::broadcast;

use crate::agents::repository::AgentRepository;
use crate::config::{SinkConfig, SourceConfig};
use crate::flows::events::{RunEvent, RunEventType};
use crate::flows::history::{FlowRun, NodeRun, RunStatus};
use crate::flows::repository::FlowRepository;
use crate::flows::{Flow, NodeType};
use crate::github::client::GithubClient;
use crate::tasks::context::render_prompt;
use crate::sandbox::provider::SandboxProvider;
use crate::api::VmMapping;
use crate::tasks::executors::Executor;
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::sandbox::SandboxExecutor;
use crate::tasks::executors::vm_executor::VmExecutor;
use crate::tasks::filters::Filter;
use crate::tasks::filters::keyword::{KeywordFilter, MatchField};
use crate::tasks::sources::{self, ContentItem};
use crate::tasks::pipeline::{format_items, resolve_sinks};

/// Data returned by `prepare_session()` — everything needed to start
/// an interactive Claude Code session for a flow.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub flow_id: String,
    pub flow_name: String,
    pub prompt: String,
    pub permissions: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub working_dir: String,
    pub sources_summary: String,
    pub sinks_summary: String,
}

pub struct FlowRunner {
    pub http_client: Arc<reqwest::Client>,
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub events_tx: Option<broadcast::Sender<RunEvent>>,
    pub sandbox_provider: Option<Arc<dyn SandboxProvider>>,
    /// VM mappings keyed by "flow_id::node_id" -> VmMapping.
    /// Used to look up web_terminal_url for vm-sandbox executors.
    pub vm_mappings: HashMap<String, VmMapping>,
    /// Agent repository for resolving `agent_id` on executor nodes.
    pub agent_repo: Option<Arc<dyn AgentRepository>>,
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
    /// Prepare a session for interactive Claude Code use.
    /// Runs sources + filters + prompt rendering but stops before executing.
    /// Returns everything the TUI needs to launch an interactive session.
    pub async fn prepare_session(&self, flow: &Flow) -> Result<SessionInfo> {
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

        // Build summaries
        let sources_summary = if source_nodes.is_empty() {
            "No sources configured".to_string()
        } else {
            let parts: Vec<String> = source_nodes
                .iter()
                .map(|n| format!("{} ({})", n.kind, n.label))
                .collect();
            format!("{} source(s): {}", source_nodes.len(), parts.join(", "))
        };

        let sinks_summary = if sink_nodes.is_empty() {
            "No sinks configured".to_string()
        } else {
            let parts: Vec<String> = sink_nodes
                .iter()
                .map(|n| format!("{} ({})", n.kind, n.label))
                .collect();
            format!("{} sink(s): {}", sink_nodes.len(), parts.join(", "))
        };

        // 1. Fetch sources
        let source_configs = parse_source_configs(&source_nodes)?;
        let github_token = self
            .github_client
            .as_ref()
            .and_then(|_| std::env::var("GITHUB_TOKEN").ok());

        let items: Vec<ContentItem> = if !source_configs.is_empty() {
            sources::fetch_all(&source_configs, &self.http_client, github_token.as_deref())
                .await
        } else {
            vec![]
        };

        // 2. Apply filters
        let mut items = items;
        for node in &filter_nodes {
            let filter = parse_filter_config(node)?;
            items = filter.apply(items);
        }

        // 3. Render prompt
        let content = format_items(&items);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content.clone());
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);

        let prompt_path = executor_node.config["prompt"]
            .as_str()
            .context("executor node missing 'prompt' config")?;

        let prompt_template = if prompt_path.ends_with(".md")
            || prompt_path.ends_with(".txt")
            || std::path::Path::new(prompt_path).exists()
        {
            std::fs::read_to_string(prompt_path)
                .with_context(|| format!("failed to read prompt file: {prompt_path}"))?
        } else {
            prompt_path.to_string()
        };

        // Fetch market data if needed
        if prompt_template.contains("{{market_data}}") {
            let market_data = match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                crate::tasks::sources::market::fetch_market_snapshot(&self.http_client),
            )
            .await
            {
                Ok(Ok(data)) => data,
                _ => "Market data unavailable.".to_string(),
            };
            vars.insert("market_data".to_string(), market_data);
        }

        let rendered = render_prompt(&prompt_template, &vars);

        let rendered = if !items.is_empty() && !prompt_template.contains("{{content}}") {
            format!(
                "{rendered}\n\n<<<\n{}\n>>>",
                vars.get("content").cloned().unwrap_or_default()
            )
        } else {
            rendered
        };

        // Extract executor config
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

        Ok(SessionInfo {
            flow_id: flow.id.clone(),
            flow_name: flow.name.clone(),
            prompt: rendered,
            permissions,
            append_system_prompt,
            working_dir: working_dir.to_string_lossy().to_string(),
            sources_summary,
            sinks_summary,
        })
    }

    /// Prepare a session for a specific executor node (node-level chat).
    /// Does NOT run sources/filters — just resolves the node's own config
    /// (prompt, permissions, working_dir, system_prompt).
    pub fn prepare_node_session(flow: &Flow, node_id: &str) -> Result<SessionInfo> {
        let executor_node = flow
            .nodes
            .iter()
            .find(|n| n.id == node_id && n.node_type == NodeType::Executor)
            .with_context(|| format!("executor node '{}' not found in flow", node_id))?;

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

        // For node-level chat the prompt is informational only — the user types
        // their own messages. We still resolve it so the UI can show it.
        let prompt = executor_node.config["prompt"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(SessionInfo {
            flow_id: flow.id.clone(),
            flow_name: flow.name.clone(),
            prompt,
            permissions,
            append_system_prompt,
            working_dir: working_dir.to_string_lossy().to_string(),
            sources_summary: "N/A (node-level chat)".into(),
            sinks_summary: "N/A (node-level chat)".into(),
        })
    }

    /// Execute a flow. If `context` is `Some`, skips source fetching/filtering
    /// and uses the provided variables for prompt rendering (e.g. PR diff).
    /// If `context` is `None`, runs the full source → filter → render pipeline.
    pub async fn execute(
        &self,
        flow: &Flow,
        repo: &dyn FlowRepository,
        context: Option<HashMap<String, String>>,
    ) -> Result<FlowRun> {
        let has_context = context.is_some();
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
        repo.add_run(run.clone()).await?;

        let ctx_label = if has_context { " (with context)" } else { "" };
        self.emit(&flow.id, &run_id, None, RunEventType::RunStarted, format!("Flow execution started{ctx_label}"));

        let span = tracing::info_span!("flow_run", flow = %flow.name, run = %short_id);
        tracing::info!(parent: &span, nodes = flow.nodes.len(), edges = flow.edges.len(), "▶ Started{ctx_label}");

        let start = std::time::Instant::now();
        let result = self.execute_inner(flow, &run_id, repo, context).instrument(span.clone()).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                repo
                    .complete_run(&flow.id, &run_id, RunStatus::Success, None)
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunCompleted, format!("Completed in {:.1}s", elapsed.as_secs_f64()));
                tracing::info!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), "✓ Completed");
            }
            Err(e) => {
                let err_msg = format!("{e:#}");
                repo
                    .complete_run(&flow.id, &run_id, RunStatus::Failed, Some(err_msg.clone()))
                    .await?;
                self.emit(&flow.id, &run_id, None, RunEventType::RunFailed, &err_msg);
                tracing::error!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), error = %err_msg, "✗ Failed");
            }
        }

        let run = repo
            .get_runs(&flow.id, 100)
            .await
            .into_iter()
            .find(|r| r.id == run_id)
            .unwrap_or(run);

        result.map(|_| run)
    }

    /// Core execution logic. When `context` is `Some`, skips source fetching
    /// and filtering — the caller provides all template variables (e.g. PR diff).
    /// When `context` is `None`, runs the full source → filter → render pipeline.
    async fn execute_inner(
        &self,
        flow: &Flow,
        run_id: &str,
        repo: &dyn FlowRepository,
        context: Option<HashMap<String, String>>,
    ) -> Result<()> {
        // Find nodes by type — use first executor for prompt resolution
        let first_executor_node = flow
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Executor)
            .context("flow has no executor node")?;

        let sink_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Sink)
            .collect();

        // ── 1–3. BUILD TEMPLATE VARS & RENDER PROMPT ────────────────
        let (rendered, _items_count) = if let Some(extra_vars) = context {
            // Context path: caller provides all template vars (e.g. PR review)
            let mut vars = extra_vars;
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
            vars.entry("timestamp".to_string()).or_insert(timestamp);

            let prompt_path = first_executor_node.config["prompt"]
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
            tracing::info!(chars = rendered.len(), "✓ Prompt rendered (with context)");

            (rendered, 0usize)
        } else {
            // Full pipeline: sources → filters → prompt rendering
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

            tracing::info!(
                "Pipeline: {} source(s) → {} filter(s) → {} → {} sink(s)",
                source_nodes.len(),
                filter_nodes.len(),
                first_executor_node.kind,
                sink_nodes.len(),
            );

            // ── 1. SOURCES ──────────────────────────────────────────
            let source_configs = parse_source_configs(&source_nodes)?;
            let github_token = self
                .github_client
                .as_ref()
                .and_then(|_| std::env::var("GITHUB_TOKEN").ok());

            let items: Vec<ContentItem> = if !source_configs.is_empty() {
                for node in &source_nodes {
                    let node_run = NodeRun {
                        node_id: node.id.clone(),
                        status: RunStatus::Running,
                        started_at: Utc::now(),
                        finished_at: None,
                        output_preview: None,
                    };
                    repo.push_node_run(&flow.id, run_id, node_run).await?;
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

                for node in &source_nodes {
                    let preview = format!("{} items fetched", result.len());
                    repo
                        .complete_node_run(&flow.id, run_id, &node.id, RunStatus::Success, Some(preview))
                        .await?;
                }

                result
            } else {
                tracing::info!("No sources configured, skipping fetch");
                vec![]
            };

            // ── 2. FILTERS ──────────────────────────────────────────
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
                repo.push_node_run(&flow.id, run_id, filter_run).await?;

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
                repo
                    .complete_node_run(&flow.id, run_id, &node.id, RunStatus::Success, Some(preview))
                    .await?;
            }

            // ── 3. PROMPT RENDERING ─────────────────────────────────
            let content = format_items(&items);
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

            let mut vars = HashMap::new();
            vars.insert("content".to_string(), content);
            vars.insert("item_count".to_string(), items.len().to_string());
            vars.insert("timestamp".to_string(), timestamp);

            let prompt_path = first_executor_node.config["prompt"]
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

            let count = items.len();
            (rendered, count)
        };

        // ── 4. EXECUTORS (sequential, edge-ordered) ────────────────
        let trigger_node = flow.nodes.iter().find(|n| n.node_type == NodeType::Trigger);

        // Collect all executor nodes in topological (edge) order
        let executor_nodes = topo_sort_executors(
            &flow.nodes.iter().filter(|n| n.node_type == NodeType::Executor).collect::<Vec<_>>(),
            &flow.edges,
        );

        if executor_nodes.is_empty() {
            bail!("flow has no executor node");
        }

        // Resolve working_dir: trigger node > first executor node > cwd
        let working_dir = trigger_node
            .and_then(|n| n.config["working_dir"].as_str())
            .or_else(|| executor_nodes[0].config["working_dir"].as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        tracing::info!(
            executor_count = executor_nodes.len(),
            "⟶ Executing {} executor(s) sequentially",
            executor_nodes.len(),
        );

        // Run each executor sequentially, piping output from one to the next
        let mut current_input = rendered;
        let mut last_result = crate::tasks::executors::ExecutionResult {
            text: String::new(),
            cost_usd: 0.0,
            num_turns: 0,
        };

        for (i, executor_node) in executor_nodes.iter().enumerate() {
            // Resolve agent config: if agent_id is set, load from agent repo; otherwise use inline config
            let (permissions, append_system_prompt) = if let Some(agent_id) =
                executor_node.config["agent_id"].as_str()
            {
                let agent = if let Some(agent_repo) = &self.agent_repo {
                    agent_repo.get(agent_id).await
                } else {
                    None
                };
                match agent {
                    Some(agent) => (agent.permissions.clone(), agent.append_system_prompt.clone()),
                    None => {
                        tracing::warn!(
                            agent_id = agent_id,
                            node = %executor_node.label,
                            "agent_id set but agent not found, falling back to inline config"
                        );
                        (
                            executor_node.config["permissions"]
                                .as_array()
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                .unwrap_or_default(),
                            executor_node.config["append_system_prompt"].as_str().map(String::from),
                        )
                    }
                }
            } else {
                (
                    executor_node.config["permissions"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    executor_node.config["append_system_prompt"].as_str().map(String::from),
                )
            };

            let has_system_prompt = append_system_prompt.is_some();

            // Dispatch on config.runtime (preferred) or fall back to node kind
            let runtime = executor_node.config["runtime"]
                .as_str()
                .unwrap_or(executor_node.kind.as_str());

            let executor: Box<dyn Executor> = match runtime {
                "sandbox" => {
                    let provider = self.sandbox_provider.as_ref()
                        .context("sandbox executor requested but no sandbox provider configured")?;
                    Box::new(SandboxExecutor::new(provider.clone(), permissions.clone(), append_system_prompt))
                }
                "vm-sandbox" => {
                    let vm_key = format!("{}::{}", flow.id, executor_node.id);
                    let mapping = self.vm_mappings.get(&vm_key)
                        .with_context(|| format!(
                            "no VM provisioned for executor node '{}' (key: {}). Enable the flow first.",
                            executor_node.label, vm_key
                        ))?;
                    tracing::info!(
                        vm_name = %mapping.vm_name,
                        vm_id = mapping.vm_id,
                        url = %mapping.web_terminal_url,
                        "using VM for executor"
                    );
                    Box::new(VmExecutor::new(
                        mapping.web_terminal_url.clone(),
                        permissions.clone(),
                        append_system_prompt,
                    ))
                }
                _ => Box::new(ClaudeCodeExecutor::new(permissions.clone(), append_system_prompt)),
            };

            let perms_display = if permissions.is_empty() { "ALL".to_string() } else { permissions.join(", ") };
            tracing::info!(
                executor = %executor_node.kind,
                step = i + 1,
                total = executor_nodes.len(),
                permissions = %perms_display,
                system_prompt = has_system_prompt,
                input_chars = current_input.len(),
                "⟶ Executing step {}/{}",
                i + 1,
                executor_nodes.len(),
            );

            let exec_node_run = NodeRun {
                node_id: executor_node.id.clone(),
                status: RunStatus::Running,
                started_at: Utc::now(),
                finished_at: None,
                output_preview: None,
            };
            repo.push_node_run(&flow.id, run_id, exec_node_run).await?;

            self.emit(&flow.id, run_id, Some(&executor_node.id), RunEventType::NodeStarted,
                format!("Executing {} (step {}/{})...", executor_node.kind, i + 1, executor_nodes.len()));

            let exec_start = std::time::Instant::now();
            let exec_result = executor
                .execute(&current_input, &working_dir)
                .await
                .with_context(|| format!("executor '{}' failed (step {}/{})", executor_node.label, i + 1, executor_nodes.len()))?;
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
                "✓ Executor step {}/{} finished",
                i + 1,
                executor_nodes.len(),
            );

            let preview = truncate(&exec_result.text, 500);
            repo
                .complete_node_run(&flow.id, run_id, &executor_node.id, RunStatus::Success, Some(preview))
                .await?;

            // Pipe output to next executor's input
            current_input = exec_result.text.clone();
            last_result = exec_result;
        }

        // Use the final executor's result for sinks
        let exec_result = last_result;

        // ── 5. SINKS ────────────────────────────────────────────────
        if !exec_result.text.is_empty() && !sink_nodes.is_empty() {
            let sink_configs = parse_sink_configs(&sink_nodes)?;
            let resolved_sinks = resolve_sinks(&sink_configs, &self.http_client)?;

            for (sink, sink_node) in resolved_sinks.iter().zip(sink_nodes.iter()) {
                let sink_run = NodeRun {
                    node_id: sink_node.id.clone(),
                    status: RunStatus::Running,
                    started_at: Utc::now(),
                    finished_at: None,
                    output_preview: None,
                };
                repo.push_node_run(&flow.id, run_id, sink_run).await?;

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

                repo
                    .complete_node_run(&flow.id, run_id, &sink_node.id, status, Some(preview))
                    .await?;
            }
        } else if exec_result.text.is_empty() {
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
            "google-sheets" => {
                let spreadsheet_id = node.config["spreadsheet_id"]
                    .as_str()
                    .context("google-sheets node missing 'spreadsheet_id'")?
                    .to_string();
                let range = node.config["range"].as_str().map(String::from);
                let service_account_key_env = node.config["service_account_key_env"].as_str().map(String::from);
                let limit = node.config["limit"].as_u64().map(|n| n as usize);
                SourceConfig::GoogleSheets {
                    spreadsheet_id, range, service_account_key_env, limit,
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

/// Sort executor nodes in topological order based on edges.
///
/// Uses edge connections between executor nodes to determine order.
/// If no edges connect executors (e.g., single executor flow), returns
/// them in their original array order.
fn topo_sort_executors<'a>(
    executor_nodes: &[&'a crate::flows::Node],
    edges: &[crate::flows::Edge],
) -> Vec<&'a crate::flows::Node> {
    use std::collections::{HashMap as StdHashMap, HashSet, VecDeque};

    if executor_nodes.len() <= 1 {
        return executor_nodes.to_vec();
    }

    // Build a set of executor node IDs for fast lookup
    let exec_ids: HashSet<&str> = executor_nodes.iter().map(|n| n.id.as_str()).collect();
    let id_to_node: StdHashMap<&str, &'a crate::flows::Node> =
        executor_nodes.iter().map(|n| (n.id.as_str(), *n)).collect();

    // Build adjacency list for executor-to-executor edges only
    let mut in_degree: StdHashMap<&str, usize> = exec_ids.iter().map(|id| (*id, 0)).collect();
    let mut adj: StdHashMap<&str, Vec<&str>> = StdHashMap::new();

    for edge in edges {
        if exec_ids.contains(edge.source.as_str()) && exec_ids.contains(edge.target.as_str()) {
            adj.entry(edge.source.as_str())
                .or_default()
                .push(edge.target.as_str());
            *in_degree.entry(edge.target.as_str()).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut sorted = Vec::new();
    while let Some(node_id) = queue.pop_front() {
        sorted.push(node_id);
        if let Some(neighbors) = adj.get(node_id) {
            for &next in neighbors {
                let deg = in_degree.get_mut(next).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }
    }

    // If topo sort didn't include all executors (no edges between them),
    // append any missing ones in their original order
    let sorted_set: HashSet<&str> = sorted.iter().copied().collect();
    for node in executor_nodes {
        if !sorted_set.contains(node.id.as_str()) {
            sorted.push(node.id.as_str());
        }
    }

    sorted
        .into_iter()
        .filter_map(|id| id_to_node.get(id).copied())
        .collect()
}
