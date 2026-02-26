/// Template gallery — loads YAML workflow templates from `static/workflows/`
/// and converts them into `Flow` structs that can be directly imported.
///
/// Directory convention: `static/workflows/{category}/{slug}.yaml`
/// Category is inferred from the parent folder name.
/// Each YAML file may include an optional `meta:` block with display metadata.
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

use crate::flows::{Edge, Flow, Node, NodeType, Position};

// ============================================================================
// Public types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub slug: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub estimated_cost: Option<String>,
    pub icon: Option<String>,
    pub pipeline_shape: PipelineShape,
    pub raw_yaml: String,
}

/// Minimal shape of the pipeline — used to render the mini flow diagram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineShape {
    pub trigger: String,
    pub sources: Vec<String>,
    pub filters: Vec<String>,
    pub executors: Vec<String>,
    pub sinks: Vec<String>,
}

// ============================================================================
// Internal YAML deserialization types
// ============================================================================

/// Top-level YAML document — everything is optional to be resilient.
#[derive(Debug, Deserialize)]
struct TemplateYaml {
    #[serde(default)]
    meta: TemplateMeta,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    trigger: Option<TriggerYaml>,
    #[serde(default)]
    sources: Vec<NodeYaml>,
    #[serde(default)]
    filters: Vec<NodeYaml>,
    #[serde(default)]
    executors: Vec<NodeYaml>,
    #[serde(default)]
    sinks: Vec<NodeYaml>,
}

#[derive(Debug, Default, Deserialize)]
struct TemplateMeta {
    title: Option<String>,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    estimated_cost: Option<String>,
    icon: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TriggerYaml {
    kind: String,
    #[serde(default)]
    config: Value,
}

#[derive(Debug, Deserialize)]
struct NodeYaml {
    kind: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    config: Value,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Public API
// ============================================================================

/// Walk `static/workflows/**/*.yaml` and return all valid templates.
/// Category is inferred from the immediate parent directory name.
pub fn load_templates(static_dir: &Path) -> Vec<TemplateMetadata> {
    let workflows_dir = static_dir.join("workflows");

    if !workflows_dir.exists() {
        tracing::warn!(
            path = %workflows_dir.display(),
            "static/workflows directory not found — no templates loaded"
        );
        return vec![];
    }

    let mut templates = Vec::new();

    // Walk category directories
    let categories = match std::fs::read_dir(&workflows_dir) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read workflows dir");
            return vec![];
        }
    };

    for cat_entry in categories.flatten() {
        let cat_path = cat_entry.path();
        if !cat_path.is_dir() {
            continue;
        }

        let category = cat_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let files = match std::fs::read_dir(&cat_path) {
            Ok(rd) => rd,
            Err(e) => {
                tracing::warn!(
                    category = %category,
                    error = %e,
                    "failed to read category directory"
                );
                continue;
            }
        };

        for file_entry in files.flatten() {
            let file_path = file_entry.path();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            let slug = file_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            match load_template_file(&file_path, &category, &slug) {
                Ok(tmpl) => templates.push(tmpl),
                Err(e) => {
                    tracing::warn!(
                        path = %file_path.display(),
                        error = %e,
                        "failed to parse template file"
                    );
                }
            }
        }
    }

    // Sort: by category alphabetically, then by title
    templates.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.title.cmp(&b.title))
    });

    tracing::info!(count = templates.len(), "loaded workflow templates");
    templates
}

/// Load and parse a single template YAML file.
pub fn load_template_file(path: &Path, category: &str, slug: &str) -> Result<TemplateMetadata> {
    let raw_yaml = std::fs::read_to_string(path)
        .with_context(|| format!("reading template file: {}", path.display()))?;

    let doc: TemplateYaml = serde_yaml::from_str(&raw_yaml)
        .with_context(|| format!("parsing template YAML: {}", path.display()))?;

    let title = doc
        .meta
        .title
        .clone()
        .or_else(|| {
            if !doc.name.is_empty() {
                Some(slug_to_title(slug))
            } else {
                None
            }
        })
        .unwrap_or_else(|| slug_to_title(slug));

    let description = doc
        .meta
        .description
        .clone()
        .or_else(|| {
            if !doc.description.is_empty() {
                Some(doc.description.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let pipeline_shape = PipelineShape {
        trigger: doc
            .trigger
            .as_ref()
            .map(|t| t.kind.clone())
            .unwrap_or_else(|| "manual".to_string()),
        sources: doc.sources.iter().map(|s| s.kind.clone()).collect(),
        filters: doc.filters.iter().map(|f| f.kind.clone()).collect(),
        executors: doc.executors.iter().map(|e| e.kind.clone()).collect(),
        sinks: doc.sinks.iter().map(|s| s.kind.clone()).collect(),
    };

    Ok(TemplateMetadata {
        slug: slug.to_string(),
        category: category.to_string(),
        title,
        description,
        tags: doc.meta.tags.clone(),
        estimated_cost: doc.meta.estimated_cost.clone(),
        icon: doc.meta.icon.clone(),
        pipeline_shape,
        raw_yaml,
    })
}

/// Convert a YAML template into a `Flow` struct ready to be stored.
/// - Assigns a new UUID as the flow id
/// - Positions nodes evenly spaced horizontally
/// - Auto-generates edges connecting the pipeline stages
/// - Sets `enabled: false` (safe default — user must explicitly enable)
pub fn parse_template_yaml(yaml: &str) -> Result<Flow> {
    let doc: TemplateYaml = serde_yaml::from_str(yaml).context("failed to parse template YAML")?;

    let now = Utc::now();
    let flow_id = Uuid::new_v4().to_string();

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    // Layout constants
    const X_STEP: f64 = 220.0;
    const Y_CENTER: f64 = 200.0;
    let mut x_cursor: f64 = 50.0;

    // ---- Trigger ----
    let trigger_id = format!("trigger-{}", short_id());
    if let Some(ref trigger) = doc.trigger {
        nodes.push(Node {
            id: trigger_id.clone(),
            node_type: NodeType::Trigger,
            kind: trigger.kind.clone(),
            config: trigger.config.clone(),
            position: Position {
                x: x_cursor,
                y: Y_CENTER,
            },
            label: label_for_trigger(&trigger.kind),
        });
        x_cursor += X_STEP;
    }

    // ---- Sources ----
    // Multiple sources are spread vertically around Y_CENTER
    let source_ids: Vec<String> = doc
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let id = format!("source-{}-{}", i + 1, short_id());
            let y_offset = if doc.sources.len() == 1 {
                Y_CENTER
            } else {
                let spread = (doc.sources.len() as f64 - 1.0) * 80.0;
                Y_CENTER - spread / 2.0 + i as f64 * 80.0
            };
            nodes.push(Node {
                id: id.clone(),
                node_type: NodeType::Source,
                kind: s.kind.clone(),
                config: s.config.clone(),
                position: Position {
                    x: x_cursor,
                    y: y_offset,
                },
                label: s.label.clone().unwrap_or_else(|| label_for_source(&s.kind)),
            });
            id
        })
        .collect();

    if !source_ids.is_empty() {
        x_cursor += X_STEP;
    }

    // ---- Filters ----
    let filter_ids: Vec<String> = doc
        .filters
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let id = format!("filter-{}-{}", i + 1, short_id());
            nodes.push(Node {
                id: id.clone(),
                node_type: NodeType::Filter,
                kind: f.kind.clone(),
                config: f.config.clone(),
                position: Position {
                    x: x_cursor,
                    y: Y_CENTER,
                },
                label: f.label.clone().unwrap_or_else(|| "Keyword Filter".into()),
            });
            id
        })
        .collect();

    if !filter_ids.is_empty() {
        x_cursor += X_STEP;
    }

    // ---- Executors ----
    let executor_ids: Vec<String> = doc
        .executors
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let id = format!("executor-{}-{}", i + 1, short_id());
            let label = e
                .label
                .clone()
                .unwrap_or_else(|| format!("Executor - E{:02}", i + 1));

            // Normalize config: if prompt is inline text, keep as-is.
            // Ensure permissions field exists (default empty array).
            let mut config = e.config.clone();
            if config.is_null() {
                config = json!({});
            }
            if config.get("permissions").is_none() {
                config["permissions"] = json!([]);
            }

            nodes.push(Node {
                id: id.clone(),
                node_type: NodeType::Executor,
                kind: e.kind.clone(),
                config,
                position: Position {
                    x: x_cursor,
                    y: Y_CENTER,
                },
                label,
            });
            x_cursor += X_STEP;
            id
        })
        .collect();

    // ---- Sinks ----
    let sink_ids: Vec<String> = doc
        .sinks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let id = format!("sink-{}-{}", i + 1, short_id());
            let y_offset = if doc.sinks.len() == 1 {
                Y_CENTER
            } else {
                let spread = (doc.sinks.len() as f64 - 1.0) * 80.0;
                Y_CENTER - spread / 2.0 + i as f64 * 80.0
            };
            nodes.push(Node {
                id: id.clone(),
                node_type: NodeType::Sink,
                kind: s.kind.clone(),
                config: s.config.clone(),
                position: Position {
                    x: x_cursor,
                    y: y_offset,
                },
                label: s.label.clone().unwrap_or_else(|| label_for_sink(&s.kind)),
            });
            id
        })
        .collect();

    // ---- Edge wiring ----
    // trigger → each source
    for src_id in &source_ids {
        edges.push(make_edge(&trigger_id, src_id));
    }
    // if no sources, trigger → first filter or first executor
    if source_ids.is_empty() {
        if let Some(first_filter) = filter_ids.first() {
            edges.push(make_edge(&trigger_id, first_filter));
        } else if let Some(first_exec) = executor_ids.first() {
            edges.push(make_edge(&trigger_id, first_exec));
        }
    }

    // sources → first filter (fan-in) or first executor
    let sources_target = filter_ids.first().or_else(|| executor_ids.first()).cloned();
    if let Some(ref tgt) = sources_target {
        for src_id in &source_ids {
            edges.push(make_edge(src_id, tgt));
        }
    }

    // filters → next filter or first executor (chain)
    for (i, fid) in filter_ids.iter().enumerate() {
        if let Some(next_filter) = filter_ids.get(i + 1) {
            edges.push(make_edge(fid, next_filter));
        } else if let Some(first_exec) = executor_ids.first() {
            edges.push(make_edge(fid, first_exec));
        }
    }

    // executors chain: E01 → E02 → E03 ...
    for i in 0..executor_ids.len().saturating_sub(1) {
        edges.push(make_edge(&executor_ids[i], &executor_ids[i + 1]));
    }

    // last executor → each sink
    if let Some(last_exec) = executor_ids.last() {
        for sink_id in &sink_ids {
            edges.push(make_edge(last_exec, sink_id));
        }
    }

    let flow_name = if !doc.name.is_empty() {
        doc.name.clone()
    } else {
        "Imported Flow".to_string()
    };

    Ok(Flow {
        id: flow_id,
        name: flow_name,
        description: doc.description.clone(),
        enabled: false, // Always disabled on import — safe default
        nodes,
        edges,
        created_at: now,
        updated_at: now,
    })
}

// ============================================================================
// Helpers
// ============================================================================

fn short_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn make_edge(source: &str, target: &str) -> Edge {
    Edge {
        id: format!(
            "e-{}-{}",
            &source[..source.len().min(8)],
            &target[..target.len().min(8)]
        ),
        source: source.to_string(),
        target: target.to_string(),
    }
}

fn slug_to_title(slug: &str) -> String {
    slug.replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn label_for_trigger(kind: &str) -> String {
    match kind {
        "cron" => "Cron Schedule".to_string(),
        "manual" => "Manual Trigger".to_string(),
        "github-pr" => "GitHub PR".to_string(),
        "webhook" => "Webhook".to_string(),
        other => slug_to_title(other),
    }
}

fn label_for_source(kind: &str) -> String {
    match kind {
        "rss" => "RSS Feed".to_string(),
        "web-scrape" => "Web Scrape".to_string(),
        "web-scraper" => "Web Scraper".to_string(),
        "github-merged-prs" => "GitHub PRs".to_string(),
        "market-data" => "Market Data".to_string(),
        other => slug_to_title(other),
    }
}

fn label_for_sink(kind: &str) -> String {
    match kind {
        "slack" => "Slack".to_string(),
        "notion" => "Notion".to_string(),
        other => slug_to_title(other),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_template() {
        let yaml = r#"
name: test-flow
description: A test flow
trigger:
  kind: cron
  config:
    schedule: "0 */4 * * *"
sources:
  - kind: rss
    label: "Test RSS"
    config:
      url: "https://example.com/feed"
      limit: 10
executors:
  - kind: claude-code
    config:
      prompt: "Summarize the news"
      permissions: []
sinks:
  - kind: slack
    config:
      webhook_url_env: SLACK_WEBHOOK_URL
"#;
        let flow = parse_template_yaml(yaml).expect("should parse");
        assert_eq!(flow.name, "test-flow");
        assert!(!flow.enabled, "imported flows must be disabled");
        assert_eq!(flow.nodes.len(), 4); // trigger + source + executor + sink
        assert!(!flow.edges.is_empty());

        // Verify node types
        let trigger_nodes: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Trigger)
            .collect();
        assert_eq!(trigger_nodes.len(), 1);
        assert_eq!(trigger_nodes[0].kind, "cron");
    }

    #[test]
    fn test_parse_template_with_meta() {
        let yaml = r#"
meta:
  title: "My Template"
  description: "A custom description"
  tags: [test, demo]
  estimated_cost: "~$0.01 / run"
  icon: "🧪"
name: my-template
trigger:
  kind: manual
  config: {}
executors:
  - kind: claude-code
    config:
      prompt: "Do something"
"#;
        let flow = parse_template_yaml(yaml).expect("should parse");
        assert_eq!(flow.name, "my-template");
        assert!(!flow.enabled);
    }

    #[test]
    fn test_parse_multi_executor() {
        let yaml = r#"
name: chained-flow
trigger:
  kind: cron
  config:
    schedule: "0 8 * * *"
sources:
  - kind: rss
    config:
      url: "https://example.com/feed"
executors:
  - kind: claude-code
    label: "E01"
    config:
      prompt: "First pass"
  - kind: claude-code
    label: "E02"
    config:
      prompt: "Second pass"
sinks:
  - kind: slack
    config:
      webhook_url_env: SLACK_WEBHOOK_URL
"#;
        let flow = parse_template_yaml(yaml).expect("should parse");
        // trigger + source + 2 executors + sink = 5
        assert_eq!(flow.nodes.len(), 5);

        // Verify executor chain edge exists
        let exec_ids: Vec<_> = flow
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Executor)
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(exec_ids.len(), 2);
        let chain_edge = flow
            .edges
            .iter()
            .find(|e| e.source == exec_ids[0] && e.target == exec_ids[1]);
        assert!(chain_edge.is_some(), "E01 → E02 edge should exist");
    }

    #[test]
    fn test_slug_to_title() {
        assert_eq!(slug_to_title("crypto-news-brief"), "Crypto News Brief");
        assert_eq!(slug_to_title("pr-review"), "Pr Review");
        assert_eq!(slug_to_title("market-brief"), "Market Brief");
    }
}
