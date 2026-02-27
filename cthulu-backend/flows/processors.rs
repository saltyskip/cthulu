use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::agents::repository::AgentRepository;
use crate::api::{FlowSessions, InteractSession, VmMapping};
use crate::config::{SinkConfig, SourceConfig};
use crate::flows::graph::NodeOutput;
use crate::flows::session_bridge::{FlowRunMeta, SessionBridge};
use crate::flows::{Node, NodeType};
use crate::github::client::GithubClient;
use crate::sandbox::provider::SandboxProvider;
use crate::tasks::context::render_prompt;
use crate::tasks::executors::{Executor, LineSink};
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::sandbox::SandboxExecutor;
use crate::tasks::executors::vm_executor::VmExecutor;
use crate::tasks::pipeline::{format_items, resolve_sinks};
use crate::tasks::sources;

/// Dependencies needed by node processors.
/// Cloneable so it can be shared across parallel tasks.
#[derive(Clone)]
pub struct NodeDeps {
    pub http_client: Arc<reqwest::Client>,
    pub github_client: Option<Arc<dyn GithubClient>>,
    pub sandbox_provider: Option<Arc<dyn SandboxProvider>>,
    pub vm_mappings: HashMap<String, VmMapping>,
    pub agent_repo: Option<Arc<dyn AgentRepository>>,
    pub flow_id: String,
    /// Session bridge for creating flow-run sessions in agent workspaces.
    pub session_bridge: Option<SessionBridge>,
    /// Current run ID (for flow-run session metadata).
    pub run_id: Option<String>,
    /// Flow name (for flow-run session metadata).
    pub flow_name: Option<String>,
}

/// Process a single node, dispatching by type.
/// Returns (NodeOutput, Option<ExecutionResult>) — the execution result is only
/// populated for executor nodes.
pub async fn process_node(
    node: &Node,
    input: NodeOutput,
    deps: &NodeDeps,
) -> Result<NodeOutput> {
    match node.node_type {
        NodeType::Trigger => Ok(NodeOutput::Empty),
        NodeType::Source => process_source(node, deps).await,
        NodeType::Executor => process_executor(node, input, deps).await,
        NodeType::Sink => process_sink(node, input, deps).await,
    }
}

// ── Source Processing ──────────────────────────────────────────────────

async fn process_source(node: &Node, deps: &NodeDeps) -> Result<NodeOutput> {
    let configs = parse_source_configs(&[node])?;
    if configs.is_empty() {
        // market-data nodes are skipped (handled via template variable)
        return Ok(NodeOutput::Empty);
    }

    let github_token = deps
        .github_client
        .as_ref()
        .and_then(|_| std::env::var("GITHUB_TOKEN").ok());

    let items = sources::fetch_all(&configs, &deps.http_client, github_token.as_deref()).await;

    tracing::debug!(
        node = %node.label,
        items = items.len(),
        "Source fetched",
    );

    Ok(NodeOutput::Items(items))
}

// ── Executor Processing ────────────────────────────────────────────────

async fn process_executor(
    node: &Node,
    input: NodeOutput,
    deps: &NodeDeps,
) -> Result<NodeOutput> {
    // Build prompt from input
    let rendered = render_executor_prompt(node, &input, deps).await?;

    // Resolve agent config
    let (permissions, append_system_prompt) = resolve_agent_config(node, deps).await?;

    // Resolve working dir
    let working_dir = node.config["working_dir"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Dispatch on runtime
    let runtime = node.config["runtime"]
        .as_str()
        .unwrap_or(node.kind.as_str());

    let executor: Box<dyn Executor> = match runtime {
        "sandbox" => {
            let provider = deps
                .sandbox_provider
                .as_ref()
                .context("sandbox executor requested but no sandbox provider configured")?;
            Box::new(SandboxExecutor::new(
                provider.clone(),
                permissions.clone(),
                append_system_prompt,
            ))
        }
        "vm-sandbox" => {
            let vm_key = format!("{}::{}", deps.flow_id, node.id);
            let mapping = deps.vm_mappings.get(&vm_key).with_context(|| {
                format!(
                    "no VM provisioned for executor node '{}' (key: {}). Enable the flow first.",
                    node.label, vm_key
                )
            })?;
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
        _ => Box::new(ClaudeCodeExecutor::new(
            permissions.clone(),
            append_system_prompt,
        )),
    };

    let perms_display = if permissions.is_empty() {
        "ALL".to_string()
    } else {
        permissions.join(", ")
    };
    tracing::info!(
        executor = %node.kind,
        permissions = %perms_display,
        input_chars = rendered.len(),
        "Executing",
    );

    // Set up session bridge for streaming into agent workspace
    let agent_id = node.config["agent_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from);
    let line_sink = setup_flow_run_session(
        &deps.session_bridge,
        &agent_id,
        &deps.flow_id,
        deps.flow_name.as_deref().unwrap_or("Unknown"),
        deps.run_id.as_deref().unwrap_or(""),
        &node.id,
        &node.label,
        &working_dir,
    )
    .await;

    let exec_result = executor
        .execute_streaming(&rendered, &working_dir, line_sink.clone())
        .await
        .with_context(|| format!("executor '{}' failed", node.label));

    // Finalize session regardless of success/failure
    finalize_flow_run_session(
        &deps.session_bridge,
        &agent_id,
        &line_sink,
        exec_result.as_ref().ok(),
    )
    .await;

    let exec_result = exec_result?;

    tracing::info!(
        turns = exec_result.num_turns,
        cost = format_args!("${:.4}", exec_result.cost_usd),
        output_chars = exec_result.text.len(),
        "Executor finished",
    );

    let text = exec_result.text.clone();
    Ok(NodeOutput::Text(text, Some(exec_result)))
}

/// Render the prompt for an executor node from its upstream input.
async fn render_executor_prompt(
    node: &Node,
    input: &NodeOutput,
    deps: &NodeDeps,
) -> Result<String> {
    // If input is Context (e.g. from GitHub PR trigger), use it as template vars
    let vars = if let Some(ctx) = input.as_context() {
        let mut vars = ctx.clone();
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        vars.entry("timestamp".to_string()).or_insert(timestamp);
        vars
    } else {
        // Build template vars from items/text
        let items = input.as_items();
        let content = if items.is_empty() {
            input.as_text()
        } else {
            format_items(&items)
        };
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content);
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);
        vars
    };

    let prompt_path = node.config["prompt"]
        .as_str()
        .context("executor node missing 'prompt' config")?;

    let prompt_template = load_prompt_template(prompt_path)?;

    // Fetch market data if needed
    let mut vars = vars;
    if prompt_template.contains("{{market_data}}") {
        let market_data = match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            crate::tasks::sources::market::fetch_market_snapshot(&deps.http_client),
        )
        .await
        {
            Ok(Ok(data)) => data,
            _ => "Market data unavailable.".to_string(),
        };
        vars.insert("market_data".to_string(), market_data);
    }

    let rendered = render_prompt(&prompt_template, &vars);

    // If we have items but the template doesn't use {{content}}, append them
    let items = input.as_items();
    let rendered = if !items.is_empty() && !prompt_template.contains("{{content}}") {
        format!(
            "{rendered}\n\n<<<\n{}\n>>>",
            vars.get("content").cloned().unwrap_or_default()
        )
    } else {
        rendered
    };

    Ok(rendered)
}

/// Resolve permissions and system prompt from the agent referenced by `agent_id`.
/// Returns an error if `agent_id` is missing or the agent cannot be found.
async fn resolve_agent_config(
    node: &Node,
    deps: &NodeDeps,
) -> Result<(Vec<String>, Option<String>)> {
    let agent_id = node.config["agent_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("executor node '{}' has no agent_id configured", node.label))?;

    let agent_repo = deps
        .agent_repo
        .as_ref()
        .context("agent repository not available")?;

    let agent = agent_repo
        .get(agent_id)
        .await
        .with_context(|| format!("agent '{}' not found (referenced by node '{}')", agent_id, node.label))?;

    Ok((agent.permissions.clone(), agent.append_system_prompt.clone()))
}

// ── Sink Processing ────────────────────────────────────────────────────

async fn process_sink(node: &Node, input: NodeOutput, deps: &NodeDeps) -> Result<NodeOutput> {
    let text = input.as_text();
    if text.is_empty() {
        tracing::warn!(node = %node.label, "Sink received empty input, skipping delivery");
        return Ok(NodeOutput::Empty);
    }

    let configs = parse_sink_configs(&[node])?;
    let resolved = resolve_sinks(&configs, &deps.http_client)?;

    for sink in &resolved {
        sink.deliver(&text)
            .await
            .with_context(|| format!("sink '{}' delivery failed", node.label))?;
    }

    tracing::info!(node = %node.label, "Sink delivered");
    Ok(NodeOutput::Empty)
}

// ── Flow-run session helpers ──────────────────────────────────────────

/// Create a flow-run session in the agent's session pool and return a LineSink
/// that writes each line to a JSONL file and broadcasts it.
#[allow(clippy::too_many_arguments)]
async fn setup_flow_run_session(
    bridge: &Option<SessionBridge>,
    agent_id: &Option<String>,
    flow_id: &str,
    flow_name: &str,
    run_id: &str,
    node_id: &str,
    node_label: &str,
    working_dir: &std::path::Path,
) -> Option<LineSink> {
    let bridge = bridge.as_ref()?;
    let agent_id = agent_id.as_ref()?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let key = format!("agent::{agent_id}");
    let meta = FlowRunMeta {
        flow_id: flow_id.to_string(),
        flow_name: flow_name.to_string(),
        run_id: run_id.to_string(),
        node_id: node_id.to_string(),
        node_label: node_label.to_string(),
    };

    // Create the session
    let session = InteractSession {
        session_id: session_id.clone(),
        summary: format!("Flow: {} — {}", flow_name, node_label),
        node_id: None,
        working_dir: working_dir.to_string_lossy().to_string(),
        active_pid: None,
        busy: true,
        message_count: 0,
        total_cost: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
        skills_dir: None,
        kind: "flow_run".to_string(),
        flow_run: Some(meta),
    };

    // Insert into session pool
    {
        let mut sessions = bridge.sessions.write().await;
        let flow_sessions = sessions
            .entry(key.clone())
            .or_insert_with(|| FlowSessions {
                flow_name: agent_id.clone(),
                active_session: String::new(),
                sessions: Vec::new(),
            });
        flow_sessions.sessions.push(session);
        // Don't change active_session — leave whatever interactive session is active
        let snapshot = sessions.clone();
        drop(sessions);
        let vms = bridge.vm_mappings.try_read()
            .map(|g| g.clone())
            .unwrap_or_default();
        crate::api::save_sessions(&bridge.sessions_path, &snapshot, &vms);
    }

    // Create broadcast channel
    let (tx, _) = tokio::sync::broadcast::channel::<String>(1024);
    {
        let mut streams = bridge.session_streams.lock().await;
        streams.insert(session_id.clone(), tx.clone());
    }

    // Ensure session_logs directory exists
    let logs_dir = bridge.data_dir.join("session_logs");
    let _ = std::fs::create_dir_all(&logs_dir);
    let log_path = logs_dir.join(format!("{session_id}.jsonl"));

    // Build the LineSink
    let log_path_clone = log_path.clone();
    let sink: LineSink = Arc::new(move |line: String| {
        // Append to JSONL file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path_clone)
        {
            let _ = writeln!(f, "{}", line);
        }
        // Broadcast (ignore errors — no subscribers is ok)
        let _ = tx.send(line);
    });

    tracing::info!(
        agent_id = %agent_id,
        session_id = %session_id,
        "Created flow-run session for executor"
    );

    Some(sink)
}

/// Finalize a flow-run session: mark as not busy, update cost/turns, remove broadcast.
async fn finalize_flow_run_session(
    bridge: &Option<SessionBridge>,
    agent_id: &Option<String>,
    line_sink: &Option<LineSink>,
    exec_result: Option<&crate::tasks::executors::ExecutionResult>,
) {
    let bridge = match bridge.as_ref() {
        Some(b) => b,
        None => return,
    };
    let agent_id = match agent_id.as_ref() {
        Some(id) => id,
        None => return,
    };
    if line_sink.is_none() {
        return;
    }

    let key = format!("agent::{agent_id}");

    // Find the flow_run session that's busy and update it
    {
        let mut sessions = bridge.sessions.write().await;
        if let Some(flow_sessions) = sessions.get_mut(&key) {
            // Find the most recently added busy flow_run session
            if let Some(session) = flow_sessions
                .sessions
                .iter_mut()
                .rev()
                .find(|s| s.kind == "flow_run" && s.busy)
            {
                session.busy = false;
                session.message_count = 1;
                if let Some(er) = exec_result {
                    session.total_cost = er.cost_usd;
                }

                // Remove broadcast sender
                let sid = session.session_id.clone();
                let mut streams = bridge.session_streams.lock().await;
                streams.remove(&sid);
            }
        }
        let snapshot = sessions.clone();
        drop(sessions);
        let vms = bridge.vm_mappings.try_read()
            .map(|g| g.clone())
            .unwrap_or_default();
        crate::api::save_sessions(&bridge.sessions_path, &snapshot, &vms);
    }
}

// ── Config Parsing Helpers (moved from runner.rs) ──────────────────────

pub fn parse_source_configs(nodes: &[&Node]) -> Result<Vec<SourceConfig>> {
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
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                SourceConfig::Rss {
                    url,
                    limit,
                    keywords,
                }
            }
            "web-scrape" => {
                let url = node.config["url"]
                    .as_str()
                    .context("web-scrape node missing 'url'")?
                    .to_string();
                let keywords = node.config["keywords"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
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
                    url,
                    base_url,
                    items_selector,
                    title_selector,
                    url_selector,
                    summary_selector,
                    date_selector,
                    date_format,
                    limit,
                }
            }
            "google-sheets" => {
                let spreadsheet_id = node.config["spreadsheet_id"]
                    .as_str()
                    .context("google-sheets node missing 'spreadsheet_id'")?
                    .to_string();
                let range = node.config["range"].as_str().map(String::from);
                let service_account_key_env = node.config["service_account_key_env"]
                    .as_str()
                    .map(String::from);
                let limit = node.config["limit"].as_u64().map(|n| n as usize);
                SourceConfig::GoogleSheets {
                    spreadsheet_id,
                    range,
                    service_account_key_env,
                    limit,
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

pub fn parse_sink_configs(nodes: &[&Node]) -> Result<Vec<SinkConfig>> {
    let mut configs = Vec::new();
    for node in nodes {
        let config = match node.kind.as_str() {
            "slack" => SinkConfig::Slack {
                webhook_url_env: node.config["webhook_url_env"].as_str().map(String::from),
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

pub fn load_prompt_template(prompt_path: &str) -> Result<String> {
    if prompt_path.ends_with(".md")
        || prompt_path.ends_with(".txt")
        || std::path::Path::new(prompt_path).exists()
    {
        std::fs::read_to_string(prompt_path)
            .with_context(|| format!("failed to read prompt file: {prompt_path}"))
    } else {
        Ok(prompt_path.to_string())
    }
}
