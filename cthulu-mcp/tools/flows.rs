//! Flow tool handlers.
//!
//! Flows are stored as JSON files in `~/.cthulu/flows/<id>.json`.
//! Each flow is a DAG: trigger → sources → (filters) → executor → sinks.
//! Node kinds per node_type:
//!   trigger  : cron | github-pr | webhook | manual
//!   source   : rss | web-scrape | web-scraper | github-merged-prs | market-data | google-sheets
//!   filter   : keyword
//!   executor : claude-code | vm-sandbox
//!   sink     : slack | notion

use rmcp::{model::CallToolResult, ErrorData as McpError};
use serde_json::Value;
use std::fmt::Write as FmtWrite;

use super::{err, ok, CthuluMcpServer};

pub async fn list_flows(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.list_flows().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_flow(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_flow(&id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn create_flow(s: &CthuluMcpServer, body: String) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.create_flow(parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn update_flow(
    s: &CthuluMcpServer,
    id: String,
    body: String,
) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.update_flow(&id, parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn delete_flow(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    s.cthulu.delete_flow(&id).await.map_err(err)?;
    ok(format!("Flow {id} deleted."))
}

pub async fn trigger_flow(
    s: &CthuluMcpServer,
    id: String,
    body: Option<String>,
) -> Result<CallToolResult, McpError> {
    let parsed = body
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|b| serde_json::from_str::<Value>(b).map_err(|e| err(format!("invalid JSON: {e}"))))
        .transpose()?;

    let v = s.cthulu.trigger_flow(&id, parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_flow_runs(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_flow_runs(&id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_flow_schedule(
    s: &CthuluMcpServer,
    id: String,
) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_flow_schedule(&id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Return a rich human-readable description of a flow's pipeline structure.
/// Includes the DAG layout, every node's kind + key config fields,
/// the executor prompt text (if the prompt is an inline string),
/// the cron schedule, sources, and sinks.
pub async fn describe_flow(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_flow(&id).await.map_err(err)?;
    ok(render_flow_description(&v))
}

/// List all workflow JSON files on disk at ~/.cthulu/flows/ with their
/// file size and last-modified timestamp, independent of the running backend.
pub async fn list_workflow_files(_s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let flows_dir = dirs::home_dir()
        .ok_or_else(|| err("cannot determine home directory"))?
        .join(".cthulu/flows");

    if !flows_dir.exists() {
        return ok(format!(
            "Workflow directory does not exist: {}",
            flows_dir.display()
        ));
    }

    let mut entries: Vec<(String, u64, String)> = Vec::new(); // (filename, bytes, mtime)

    let read_dir = std::fs::read_dir(&flows_dir)
        .map_err(|e| err(format!("cannot read {}: {e}", flows_dir.display())))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| err(format!("directory read error: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let meta = std::fs::metadata(&path)
            .map_err(|e| err(format!("metadata error: {e}")))?;
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok()
            })
            .map(|d| {
                // Format as YYYY-MM-DD HH:MM UTC
                let secs = d.as_secs();
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                    .unwrap_or_default();
                dt.format("%Y-%m-%d %H:%M UTC").to_string()
            })
            .unwrap_or_else(|| "unknown".into());

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        entries.push((name, size, mtime));
    }

    entries.sort_by(|a, b| b.2.cmp(&a.2)); // newest first

    if entries.is_empty() {
        return ok(format!(
            "No workflow files found in {}",
            flows_dir.display()
        ));
    }

    let mut out = format!(
        "Workflow files in {}:\n\n",
        flows_dir.display()
    );
    for (name, size, mtime) in &entries {
        let _ = writeln!(out, "  {name}  ({size} bytes, modified {mtime})");
    }
    out.push_str(&format!("\nTotal: {} file(s)\n", entries.len()));
    out.push_str("\nTip: Use get_flow with the UUID (filename without .json) to inspect a flow.");

    ok(out)
}

// ── Rendering helpers ─────────────────────────────────────────────────────────

fn render_flow_description(v: &Value) -> String {
    let mut out = String::new();

    // Header
    let name = str_field(v, "name");
    let id = str_field(v, "id");
    let enabled = v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
    let description = str_field(v, "description");
    let updated = str_field(v, "updated_at");

    let _ = writeln!(out, "# {name}");
    let _ = writeln!(out, "id       : {id}");
    let _ = writeln!(out, "enabled  : {enabled}");
    if !description.is_empty() {
        let _ = writeln!(out, "description: {description}");
    }
    let _ = writeln!(out, "updated  : {updated}");
    let _ = writeln!(out);

    let nodes = v.get("nodes").and_then(|n| n.as_array()).cloned().unwrap_or_default();
    let edges = v.get("edges").and_then(|e| e.as_array()).cloned().unwrap_or_default();

    // Count by type
    let count = |t: &str| nodes.iter().filter(|n| str_field(n, "node_type") == t).count();
    let _ = writeln!(
        out,
        "## Pipeline shape: {} trigger(s) → {} source(s) → {} executor(s) → {} sink(s)  ({} edge(s))",
        count("trigger"), count("source"), count("executor"), count("sink"), edges.len()
    );
    let _ = writeln!(out);

    // Triggers
    let triggers: Vec<_> = nodes.iter().filter(|n| str_field(n, "node_type") == "trigger").collect();
    if !triggers.is_empty() {
        let _ = writeln!(out, "### Trigger(s)");
        for n in &triggers {
            render_node(&mut out, n);
        }
    }

    // Sources
    let sources: Vec<_> = nodes.iter().filter(|n| str_field(n, "node_type") == "source").collect();
    if !sources.is_empty() {
        let _ = writeln!(out, "### Source(s)");
        for n in &sources {
            render_node(&mut out, n);
        }
    }

    // Filters
    let filters: Vec<_> = nodes.iter().filter(|n| str_field(n, "node_type") == "filter").collect();
    if !filters.is_empty() {
        let _ = writeln!(out, "### Filter(s)");
        for n in &filters {
            render_node(&mut out, n);
        }
    }

    // Executors
    let executors: Vec<_> = nodes.iter().filter(|n| str_field(n, "node_type") == "executor").collect();
    if !executors.is_empty() {
        let _ = writeln!(out, "### Executor(s)");
        for n in &executors {
            render_node(&mut out, n);
            // Show prompt text inline if it looks like a path
            let prompt_str_opt = n
                .get("config")
                .and_then(|cfg| cfg.get("prompt"))
                .and_then(|p| p.as_str())
                .filter(|s| s.ends_with(".md") || s.ends_with(".txt"));
            if let Some(prompt_str) = prompt_str_opt {
                let candidates = &[
                    std::path::PathBuf::from(prompt_str),
                    dirs::home_dir().unwrap_or_default().join(".cthulu").join(prompt_str),
                    std::env::current_dir().unwrap_or_default().join(prompt_str),
                ];
                for path in candidates {
                    if let Ok(text) = std::fs::read_to_string(path) {
                        let preview: String = text.chars().take(600).collect();
                        let _ = writeln!(out, "  prompt preview (first 600 chars):");
                        let _ = writeln!(out, "  ---");
                        for line in preview.lines() {
                            let _ = writeln!(out, "  {line}");
                        }
                        let _ = writeln!(out, "  ---");
                        break;
                    }
                }
            }
        }
    }

    // Sinks
    let sinks: Vec<_> = nodes.iter().filter(|n| str_field(n, "node_type") == "sink").collect();
    if !sinks.is_empty() {
        let _ = writeln!(out, "### Sink(s)");
        for n in &sinks {
            render_node(&mut out, n);
        }
    }

    // Edge list (compact)
    if !edges.is_empty() {
        let _ = writeln!(out, "### Edges (source_id → target_id)");
        // Build an id→label map for readability
        let label_map: std::collections::HashMap<String, String> = nodes
            .iter()
            .map(|n| {
                let nid = str_field(n, "id");
                let lbl = str_field(n, "label");
                let kind = str_field(n, "kind");
                let display = if lbl.is_empty() { kind } else { lbl };
                (nid, display)
            })
            .collect();

        for e in &edges {
            let src = str_field(e, "source");
            let tgt = str_field(e, "target");
            let src_lbl = label_map.get(&src).cloned().unwrap_or_else(|| src.clone());
            let tgt_lbl = label_map.get(&tgt).cloned().unwrap_or_else(|| tgt.clone());
            let _ = writeln!(out, "  {src_lbl} → {tgt_lbl}");
        }
    }

    out
}

fn render_node(out: &mut String, n: &Value) {
    let label = str_field(n, "label");
    let kind = str_field(n, "kind");
    let id = str_field(n, "id");
    let _ = writeln!(out, "  [{kind}] {label}  (id: {id})");

    if let Some(obj) = n.get("config").and_then(|cfg| cfg.as_object()) {
        for (k, val) in obj {
            // Skip large/noisy fields
            if k == "working_dir" && val.as_str() == Some(".") {
                continue;
            }
            let display = match val {
                Value::String(s) => s.clone(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.to_string(),
            };
            if !display.is_empty() {
                let _ = writeln!(out, "    {k}: {display}");
            }
        }
    }
    let _ = writeln!(out);
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string()
}
