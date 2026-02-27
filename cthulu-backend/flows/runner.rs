use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::Instrument;
use uuid::Uuid;

use tokio::sync::broadcast;

use crate::agents::repository::AgentRepository;
use crate::flows::events::{RunEvent, RunEventType};
use crate::flows::graph::{self, NodeOutput};
use crate::flows::history::{FlowRun, NodeRun, RunStatus};
use crate::flows::processors::{self, NodeDeps};
use crate::flows::repository::FlowRepository;
use crate::flows::session_bridge::SessionBridge;
use crate::flows::{Flow, NodeType};
use crate::github::client::GithubClient;
use crate::sandbox::provider::SandboxProvider;
use crate::api::VmMapping;
use crate::tasks::context::render_prompt;
use crate::tasks::pipeline::format_items;
use crate::tasks::sources::{self, ContentItem};

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
    /// Session bridge for routing executor output to agent workspaces.
    pub session_bridge: Option<SessionBridge>,
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
    /// Runs sources + prompt rendering but stops before executing.
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
        let source_configs = processors::parse_source_configs(&source_nodes)?;
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

        // 2. Render prompt
        let content = format_items(&items);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        let mut vars = HashMap::new();
        vars.insert("content".to_string(), content.clone());
        vars.insert("item_count".to_string(), items.len().to_string());
        vars.insert("timestamp".to_string(), timestamp);

        let prompt_path = executor_node.config["prompt"]
            .as_str()
            .context("executor node missing 'prompt' config")?;

        let prompt_template = processors::load_prompt_template(prompt_path)?;

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

        // Resolve permissions and system prompt from the referenced agent
        let (permissions, append_system_prompt) = if let Some(agent_id) =
            executor_node.config["agent_id"].as_str().filter(|s| !s.is_empty())
        {
            if let Some(repo) = &self.agent_repo {
                if let Some(agent) = repo.get(agent_id).await {
                    (agent.permissions.clone(), agent.append_system_prompt.clone())
                } else {
                    tracing::warn!(agent_id, node = %executor_node.label, "agent not found, using empty config");
                    (vec![], None)
                }
            } else {
                (vec![], None)
            }
        } else {
            (vec![], None)
        };

        let working_dir = executor_node.config["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

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
    /// Does NOT run sources — just resolves the node's own config
    /// (prompt, permissions, working_dir, system_prompt).
    /// Permissions and system prompt are resolved from the referenced agent.
    pub async fn prepare_node_session(
        flow: &Flow,
        node_id: &str,
        agent_repo: Option<&Arc<dyn AgentRepository>>,
    ) -> Result<SessionInfo> {
        let executor_node = flow
            .nodes
            .iter()
            .find(|n| n.id == node_id && n.node_type == NodeType::Executor)
            .with_context(|| format!("executor node '{}' not found in flow", node_id))?;

        // Resolve permissions and system prompt from the referenced agent
        let (permissions, append_system_prompt) = if let Some(agent_id) =
            executor_node.config["agent_id"].as_str().filter(|s| !s.is_empty())
        {
            if let Some(repo) = agent_repo {
                if let Some(agent) = repo.get(agent_id).await {
                    (agent.permissions.clone(), agent.append_system_prompt.clone())
                } else {
                    tracing::warn!(agent_id, node = %executor_node.label, "agent not found, using empty config");
                    (vec![], None)
                }
            } else {
                (vec![], None)
            }
        } else {
            (vec![], None)
        };

        let working_dir = executor_node.config["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

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

    /// Execute a flow. If `context` is `Some`, skips source fetching
    /// and uses the provided variables for prompt rendering (e.g. PR diff).
    /// If `context` is `None`, runs the full source → render pipeline.
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

        // Determine final status: if execute_inner returned Ok but any node failed, mark as Failed
        let (final_status, final_error) = match &result {
            Ok(any_failed) => {
                if *any_failed {
                    (RunStatus::Failed, Some("one or more nodes failed".to_string()))
                } else {
                    (RunStatus::Success, None)
                }
            }
            Err(e) => (RunStatus::Failed, Some(format!("{e:#}"))),
        };

        repo.complete_run(&flow.id, &run_id, final_status, final_error.clone()).await?;

        match final_status {
            RunStatus::Success => {
                self.emit(&flow.id, &run_id, None, RunEventType::RunCompleted, format!("Completed in {:.1}s", elapsed.as_secs_f64()));
                tracing::info!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), "✓ Completed");
            }
            _ => {
                let err_msg = final_error.as_deref().unwrap_or("unknown error");
                self.emit(&flow.id, &run_id, None, RunEventType::RunFailed, err_msg);
                tracing::error!(parent: &span, elapsed = format_args!("{:.1}s", elapsed.as_secs_f64()), error = %err_msg, "✗ Failed");
            }
        }

        let run = repo
            .get_runs(&flow.id, 100)
            .await
            .into_iter()
            .find(|r| r.id == run_id)
            .unwrap_or(run);

        // If execute_inner itself errored (not just node failures), propagate
        if let Err(e) = result {
            return Err(e);
        }

        Ok(run)
    }

    /// Core DAG execution engine.
    ///
    /// Topologically sorts all nodes, groups them by level (distance from roots),
    /// and executes each level in parallel. Edges determine data flow — each node
    /// receives the merged output of its parents.
    ///
    /// Returns Ok(true) if any node failed (but independent branches completed),
    /// Ok(false) if all nodes succeeded, or Err if there's a structural problem.
    async fn execute_inner(
        &self,
        flow: &Flow,
        run_id: &str,
        repo: &dyn FlowRepository,
        context: Option<HashMap<String, String>>,
    ) -> Result<bool> {
        // Topo sort all nodes
        let sorted = graph::topo_sort(&flow.nodes, &flow.edges)?;
        let (_, parents) = graph::build_adjacency(&flow.nodes, &flow.edges);
        let levels = graph::compute_levels(&sorted, &parents);

        // Build node lookup
        let node_map: HashMap<&str, &crate::flows::Node> =
            flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        // Per-node output storage
        let mut outputs: HashMap<String, NodeOutput> = HashMap::new();

        // Inject context as trigger output if provided (GitHub PR path)
        if let Some(ctx) = context {
            if let Some(trigger) = flow.nodes.iter().find(|n| n.node_type == NodeType::Trigger) {
                outputs.insert(trigger.id.clone(), NodeOutput::Context(ctx));
            }
        }

        let deps = NodeDeps {
            http_client: Arc::clone(&self.http_client),
            github_client: self.github_client.clone(),
            sandbox_provider: self.sandbox_provider.clone(),
            vm_mappings: self.vm_mappings.clone(),
            agent_repo: self.agent_repo.clone(),
            flow_id: flow.id.clone(),
            session_bridge: self.session_bridge.clone(),
            run_id: Some(run_id.to_string()),
            flow_name: Some(flow.name.clone()),
        };

        let mut any_failed = false;

        for level in &levels {
            // For nodes within a level that can run in parallel, we collect futures
            // However, since nodes in the same level are independent (no edges between them),
            // we can process them concurrently
            let mut handles: Vec<(String, tokio::task::JoinHandle<Result<NodeOutput>>)> = Vec::new();

            for node_id in level {
                let node = match node_map.get(node_id.as_str()) {
                    Some(n) => *n,
                    None => continue,
                };

                // Triggers: just mark as Empty if no context was injected
                if node.node_type == NodeType::Trigger {
                    outputs.entry(node_id.clone()).or_insert(NodeOutput::Empty);
                    continue;
                }

                // Collect & merge parent outputs
                let parent_outputs: Vec<NodeOutput> = parents
                    .get(node_id.as_str())
                    .map(|pids| {
                        pids.iter()
                            .filter_map(|p| outputs.get(p).cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                let input = NodeOutput::merge(parent_outputs);

                // Skip if any parent failed (propagate failure sentinel)
                if matches!(input, NodeOutput::Failed) {
                    outputs.insert(node_id.clone(), NodeOutput::Failed);
                    any_failed = true;
                    tracing::warn!(node = %node.label, "Skipping node — upstream failed");
                    continue;
                }

                // Record node run start
                let node_run = NodeRun {
                    node_id: node_id.clone(),
                    status: RunStatus::Running,
                    started_at: Utc::now(),
                    finished_at: None,
                    output_preview: None,
                };
                repo.push_node_run(&flow.id, run_id, node_run).await?;
                self.emit(
                    &flow.id,
                    run_id,
                    Some(node_id),
                    RunEventType::NodeStarted,
                    format!("Processing {}...", node.label),
                );

                // Spawn task for parallel execution within the level
                let node_clone = node.clone();
                let deps_clone = deps.clone();
                let handle = tokio::spawn(async move {
                    processors::process_node(&node_clone, input, &deps_clone).await
                });
                handles.push((node_id.clone(), handle));
            }

            // Await all parallel tasks in this level
            for (node_id, handle) in handles {
                let node = node_map[node_id.as_str()];
                match handle.await {
                    Ok(Ok(output)) => {
                        // Build preview for node run
                        let preview = match &output {
                            NodeOutput::Items(items) => format!("{} items", items.len()),
                            NodeOutput::Text(t, exec_result) => {
                                if let Some(er) = exec_result {
                                    self.emit(
                                        &flow.id,
                                        run_id,
                                        Some(&node_id),
                                        RunEventType::NodeCompleted,
                                        format!(
                                            "{} turns, ${:.4}, {} chars output",
                                            er.num_turns, er.cost_usd, er.text.len()
                                        ),
                                    );
                                }
                                truncate(t, 500)
                            }
                            NodeOutput::Empty => "Done".to_string(),
                            _ => "Done".to_string(),
                        };

                        if !matches!(output, NodeOutput::Text(_, Some(_))) {
                            self.emit(
                                &flow.id,
                                run_id,
                                Some(&node_id),
                                RunEventType::NodeCompleted,
                                &preview,
                            );
                        }

                        tracing::info!(node = %node.label, "✓ Node completed");
                        repo.complete_node_run(
                            &flow.id,
                            run_id,
                            &node_id,
                            RunStatus::Success,
                            Some(preview),
                        )
                        .await?;
                        outputs.insert(node_id, output);
                    }
                    Ok(Err(e)) => {
                        let err_msg = format!("{e:#}");
                        self.emit(
                            &flow.id,
                            run_id,
                            Some(&node_id),
                            RunEventType::NodeFailed,
                            &err_msg,
                        );
                        tracing::error!(node = %node.label, error = %err_msg, "✗ Node failed");
                        repo.complete_node_run(
                            &flow.id,
                            run_id,
                            &node_id,
                            RunStatus::Failed,
                            Some(err_msg),
                        )
                        .await?;
                        outputs.insert(node_id, NodeOutput::Failed);
                        any_failed = true;
                    }
                    Err(join_err) => {
                        let err_msg = format!("task panicked: {join_err}");
                        self.emit(
                            &flow.id,
                            run_id,
                            Some(&node_id),
                            RunEventType::NodeFailed,
                            &err_msg,
                        );
                        tracing::error!(node = %node.label, error = %err_msg, "✗ Node panicked");
                        repo.complete_node_run(
                            &flow.id,
                            run_id,
                            &node_id,
                            RunStatus::Failed,
                            Some(err_msg),
                        )
                        .await?;
                        outputs.insert(node_id, NodeOutput::Failed);
                        any_failed = true;
                    }
                }
            }
        }

        Ok(any_failed)
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
