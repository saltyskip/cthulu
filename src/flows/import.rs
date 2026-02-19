use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::config::{
    CronTriggerConfig, ExecutorType, GithubTriggerConfig, SourceConfig, TaskConfig,
};
use crate::flows::storage::FlowStore;
use crate::flows::{Edge, Flow, Node, NodeType, Position};

const NODE_SPACING_X: f64 = 280.0;
const NODE_SPACING_Y: f64 = 120.0;

pub async fn import_toml_tasks(tasks: &[TaskConfig], store: &FlowStore) -> Result<usize> {
    let mut count = 0;

    for task in tasks {
        let flow = task_to_flow(task);
        tracing::info!(
            task = %task.name,
            flow_id = %flow.id,
            nodes = flow.nodes.len(),
            edges = flow.edges.len(),
            "Imported TOML task as flow"
        );
        store.save(flow).await?;
        count += 1;
    }

    Ok(count)
}

fn task_to_flow(task: &TaskConfig) -> Flow {
    let now = Utc::now();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut x = 0.0;

    // 1. Trigger node
    let trigger_id = Uuid::new_v4().to_string();
    let (trigger_kind, trigger_config, trigger_label) = build_trigger_node(task);
    nodes.push(Node {
        id: trigger_id.clone(),
        node_type: NodeType::Trigger,
        kind: trigger_kind,
        config: trigger_config,
        position: Position { x, y: 0.0 },
        label: trigger_label,
    });
    x += NODE_SPACING_X;

    // 2. Source nodes
    let source_ids: Vec<String> = task
        .sources
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let source_id = Uuid::new_v4().to_string();
            let y_offset = (i as f64 - (task.sources.len() as f64 - 1.0) / 2.0) * NODE_SPACING_Y;
            let (kind, config, label) = build_source_node(source);

            nodes.push(Node {
                id: source_id.clone(),
                node_type: NodeType::Source,
                kind,
                config,
                position: Position {
                    x,
                    y: y_offset,
                },
                label,
            });

            edges.push(Edge {
                id: Uuid::new_v4().to_string(),
                source: trigger_id.clone(),
                target: source_id.clone(),
            });

            source_id
        })
        .collect();

    if !task.sources.is_empty() {
        x += NODE_SPACING_X;
    }

    // 3. Executor node
    let executor_id = Uuid::new_v4().to_string();
    let (exec_kind, exec_config, exec_label) = build_executor_node(task);
    nodes.push(Node {
        id: executor_id.clone(),
        node_type: NodeType::Executor,
        kind: exec_kind,
        config: exec_config,
        position: Position { x, y: 0.0 },
        label: exec_label,
    });

    // Connect sources → executor (or trigger → executor if no sources)
    if source_ids.is_empty() {
        edges.push(Edge {
            id: Uuid::new_v4().to_string(),
            source: trigger_id,
            target: executor_id.clone(),
        });
    } else {
        for sid in &source_ids {
            edges.push(Edge {
                id: Uuid::new_v4().to_string(),
                source: sid.clone(),
                target: executor_id.clone(),
            });
        }
    }
    x += NODE_SPACING_X;

    // 4. Sink nodes
    for (i, sink_config) in task.sinks.iter().enumerate() {
        let sink_id = Uuid::new_v4().to_string();
        let y_offset = (i as f64 - (task.sinks.len() as f64 - 1.0) / 2.0) * NODE_SPACING_Y;
        let (kind, config, label) = build_sink_node(sink_config);

        nodes.push(Node {
            id: sink_id.clone(),
            node_type: NodeType::Sink,
            kind,
            config,
            position: Position {
                x,
                y: y_offset,
            },
            label,
        });

        edges.push(Edge {
            id: Uuid::new_v4().to_string(),
            source: executor_id.clone(),
            target: sink_id,
        });
    }

    Flow {
        id: Uuid::new_v4().to_string(),
        name: task.name.clone(),
        description: format!("Imported from cthulu.toml task '{}'", task.name),
        enabled: true,
        nodes,
        edges,
        created_at: now,
        updated_at: now,
    }
}

fn build_trigger_node(task: &TaskConfig) -> (String, serde_json::Value, String) {
    if let Some(gh) = &task.trigger.github {
        build_github_trigger(gh)
    } else if let Some(cron) = &task.trigger.cron {
        build_cron_trigger(cron)
    } else if let Some(wh) = &task.trigger.webhook {
        (
            "webhook".to_string(),
            json!({ "path": wh.path }),
            format!("Webhook: {}", wh.path),
        )
    } else {
        (
            "manual".to_string(),
            json!({}),
            "Manual Trigger".to_string(),
        )
    }
}

fn build_cron_trigger(cron: &CronTriggerConfig) -> (String, serde_json::Value, String) {
    let label = human_readable_cron(&cron.schedule);
    (
        "cron".to_string(),
        json!({
            "schedule": cron.schedule,
            "working_dir": cron.working_dir.display().to_string(),
        }),
        label,
    )
}

fn build_github_trigger(gh: &GithubTriggerConfig) -> (String, serde_json::Value, String) {
    let repos: Vec<_> = gh.repos.iter().map(|r| &r.slug).collect();
    let label = if repos.len() == 1 {
        format!("PR: {}", repos[0])
    } else {
        format!("PR: {} repos", repos.len())
    };
    (
        "github-pr".to_string(),
        json!({
            "repos": gh.repos.iter().map(|r| json!({
                "slug": r.slug,
                "path": r.path.display().to_string(),
            })).collect::<Vec<_>>(),
            "poll_interval": gh.poll_interval,
            "skip_drafts": gh.skip_drafts,
            "review_on_push": gh.review_on_push,
            "max_diff_size": gh.max_diff_size,
        }),
        label,
    )
}

fn build_source_node(
    source: &SourceConfig,
) -> (String, serde_json::Value, String) {
    match source {
        SourceConfig::Rss { url, limit, keywords } => {
            let domain = url
                .split("//")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or(url);
            (
                "rss".to_string(),
                json!({ "url": url, "limit": limit, "keywords": keywords }),
                format!("RSS: {domain}"),
            )
        }
        SourceConfig::WebScrape { url, keywords } => {
            let domain = url
                .split("//")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or(url);
            (
                "web-scrape".to_string(),
                json!({ "url": url, "keywords": keywords }),
                format!("Scrape: {domain}"),
            )
        }
        SourceConfig::GithubMergedPrs { repos, since_days } => (
            "github-merged-prs".to_string(),
            json!({ "repos": repos, "since_days": since_days }),
            format!("Merged PRs: {} repos", repos.len()),
        ),
        SourceConfig::WebScraper {
            url, base_url, items_selector, title_selector,
            url_selector, summary_selector, date_selector,
            date_format, limit,
        } => {
            let domain = url
                .split("//")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or(url);
            (
                "web-scraper".to_string(),
                json!({
                    "url": url,
                    "base_url": base_url,
                    "items_selector": items_selector,
                    "title_selector": title_selector,
                    "url_selector": url_selector,
                    "summary_selector": summary_selector,
                    "date_selector": date_selector,
                    "date_format": date_format,
                    "limit": limit,
                }),
                format!("Scrape: {domain}"),
            )
        }
    }
}

fn build_executor_node(task: &TaskConfig) -> (String, serde_json::Value, String) {
    let kind = match task.executor {
        ExecutorType::ClaudeCode => "claude-code",
        ExecutorType::ClaudeApi => "claude-api",
    };
    (
        kind.to_string(),
        json!({
            "prompt": task.prompt,
            "permissions": task.permissions,
            "append_system_prompt": task.append_system_prompt,
        }),
        format!("Claude: {}", task.prompt),
    )
}

fn build_sink_node(
    sink: &crate::config::SinkConfig,
) -> (String, serde_json::Value, String) {
    match sink {
        crate::config::SinkConfig::Slack {
            webhook_url_env,
            bot_token_env,
            channel,
        } => {
            let label = channel
                .as_deref()
                .map(|c| format!("Slack: {c}"))
                .unwrap_or_else(|| "Slack Webhook".to_string());
            (
                "slack".to_string(),
                json!({
                    "webhook_url_env": webhook_url_env,
                    "bot_token_env": bot_token_env,
                    "channel": channel,
                }),
                label,
            )
        }
        crate::config::SinkConfig::Notion {
            token_env,
            database_id,
        } => (
            "notion".to_string(),
            json!({
                "token_env": token_env,
                "database_id": database_id,
            }),
            format!("Notion: {}...", &database_id[..8.min(database_id.len())]),
        ),
    }
}

fn human_readable_cron(schedule: &str) -> String {
    let parts: Vec<&str> = schedule.split_whitespace().collect();
    if parts.len() != 5 {
        return format!("Cron: {schedule}");
    }

    let (min, hour, _dom, _month, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // Common patterns
    if hour.starts_with("*/") {
        let interval = &hour[2..];
        return format!("Every {interval}h");
    }
    if dow == "MON" || dow == "1" {
        return format!("Mondays at {hour}:{min:0>2}");
    }
    if dow == "*" && min == "0" {
        return format!("Daily at {hour}:00");
    }

    format!("Cron: {schedule}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::path::PathBuf;

    fn make_cron_task() -> TaskConfig {
        TaskConfig {
            name: "test-task".to_string(),
            executor: ExecutorType::ClaudeCode,
            prompt: "prompts/test.md".to_string(),
            permissions: vec![],
            trigger: TriggerConfig {
                github: None,
                cron: Some(CronTriggerConfig {
                    schedule: "0 */4 * * *".to_string(),
                    working_dir: PathBuf::from("."),
                }),
                webhook: None,
            },
            sources: vec![
                SourceConfig::Rss {
                    url: "https://example.com/feed".to_string(),
                    limit: 10,
                    keywords: vec![],
                },
            ],
            append_system_prompt: None,
            sinks: vec![SinkConfig::Notion {
                token_env: "TOKEN".to_string(),
                database_id: "abc123def456".to_string(),
            }],
        }
    }

    #[test]
    fn test_task_to_flow() {
        let task = make_cron_task();
        let flow = task_to_flow(&task);

        assert_eq!(flow.name, "test-task");
        assert!(flow.enabled);
        // trigger + 1 source + executor + 1 sink = 4 nodes
        assert_eq!(flow.nodes.len(), 4);
        // trigger->source, source->executor, executor->sink = 3 edges
        assert_eq!(flow.edges.len(), 3);

        let trigger = flow.nodes.iter().find(|n| n.node_type == NodeType::Trigger).unwrap();
        assert_eq!(trigger.kind, "cron");

        let source = flow.nodes.iter().find(|n| n.node_type == NodeType::Source).unwrap();
        assert_eq!(source.kind, "rss");

        let executor = flow.nodes.iter().find(|n| n.node_type == NodeType::Executor).unwrap();
        assert_eq!(executor.kind, "claude-code");

        let sink = flow.nodes.iter().find(|n| n.node_type == NodeType::Sink).unwrap();
        assert_eq!(sink.kind, "notion");
    }

    #[test]
    fn test_human_readable_cron() {
        assert_eq!(human_readable_cron("0 */4 * * *"), "Every 4h");
        assert_eq!(human_readable_cron("0 8 * * *"), "Daily at 8:00");
        assert_eq!(human_readable_cron("0 9 * * MON"), "Mondays at 9:00");
    }

    #[test]
    fn test_task_without_sources() {
        let task = TaskConfig {
            name: "no-sources".to_string(),
            executor: ExecutorType::ClaudeCode,
            prompt: "test.md".to_string(),
            permissions: vec!["Bash".to_string()],
            trigger: TriggerConfig {
                github: Some(GithubTriggerConfig {
                    event: "pull_request".to_string(),
                    repos: vec![RepoEntry {
                        slug: "owner/repo".to_string(),
                        path: PathBuf::from("/tmp"),
                    }],
                    poll_interval: 60,
                    skip_drafts: true,
                    review_on_push: false,
                    max_diff_size: 50000,
                }),
                cron: None,
                webhook: None,
            },
            sources: vec![],
            sinks: vec![],
            append_system_prompt: None,
        };

        let flow = task_to_flow(&task);
        // trigger + executor = 2 nodes
        assert_eq!(flow.nodes.len(), 2);
        // trigger -> executor = 1 edge
        assert_eq!(flow.edges.len(), 1);
    }
}
