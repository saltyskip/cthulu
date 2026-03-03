use rmcp::{model::CallToolResult, ErrorData as McpError};

use super::{err, ok, CthuluMcpServer};

/// Hardcoded schema — mirrors cthulu-backend/flows/mod.rs NodeType + pipeline.rs dispatch.
/// Returned as a reference when the backend is unavailable, or supplemented by get_node_types.
pub const NODE_SCHEMA: &str = r##"
Cthulu node schema (all node_type values are lowercase):

node_type: trigger
  kind: cron
    config: { schedule: "<5-field cron>", working_dir?: "." }
  kind: github-pr
    config: { repo: "owner/repo", working_dir?: "." }
  kind: webhook
    config: { working_dir?: "." }
  kind: manual
    config: { working_dir?: "." }

node_type: source
  kind: rss
    config: { url: "<feed url>", limit?: 20, keywords?: ["word1","word2"] }
  kind: web-scrape
    config: { url: "<page url>", keywords?: ["word1"] }
  kind: web-scraper
    config: { url: "<page url>", items_selector: "css", title_selector: "css", url_selector: "css" }
  kind: github-merged-prs
    config: { repos: ["owner/repo"], since_days?: 7 }
  kind: market-data
    config: {}   (no config needed -- fetches BTC/ETH/S&P/Fear&Greed automatically)
  kind: google-sheets
    config: { spreadsheet_id: "...", range: "Sheet1!A1:Z100", service_account_key_env: "ENV_VAR", limit?: 100 }

node_type: filter
  kind: keyword
    config: { keywords: ["word1","word2"], mode?: "any"|"all", field?: "title"|"content" }

node_type: executor
  kind: claude-code
    config: { prompt: "<inline text or path/to/prompt.md>", permissions?: ["Bash","Read","Grep","Glob"], working_dir?: "." }
  kind: vm-sandbox
    config: { working_dir?: "." }

node_type: sink
  kind: slack
    config: { webhook_url_env?: "SLACK_WEBHOOK_URL", bot_token_env?: "SLACK_BOT_TOKEN", channel?: "#general" }
  kind: notion
    config: { token_env: "NOTION_TOKEN", database_id: "<notion db uuid>" }

Prompt template variables (used inside executor prompts):
  {{content}}      -- formatted source items
  {{item_count}}   -- number of items fetched
  {{timestamp}}    -- current UTC timestamp
  {{market_data}}  -- crypto/market snapshot (only if market-data source is connected)
  {{diff}}         -- PR diff (only for github-pr trigger)
  {{pr_number}}, {{pr_title}}, {{repo}} -- GitHub PR context

Workflow files are stored at: ~/.cthulu/flows/<uuid>.json
"##;

pub async fn list_templates(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.list_templates().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn import_template(
    s: &CthuluMcpServer,
    category: String,
    slug: String,
) -> Result<CallToolResult, McpError> {
    let v = s
        .cthulu
        .import_template(&category, &slug)
        .await
        .map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn validate_cron(
    s: &CthuluMcpServer,
    expression: String,
) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.validate_cron(&expression).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_scheduler_status(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_scheduler_status().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_token_status(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_token_status().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Return the node type schema: all valid node_type values, their kinds,
/// and the config fields each kind accepts. Combines the live backend
/// /api/node-types response with the hardcoded reference schema.
pub async fn get_node_types(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    // Try to get live schema from backend (richer, includes labels/descriptions)
    match s.cthulu.list_node_types().await {
        Ok(v) => {
            let live = serde_json::to_string_pretty(&v).unwrap_or_default();
            let combined = format!(
                "## Live node types from backend\n\n{live}\n\n## Full schema reference\n{NODE_SCHEMA}"
            );
            ok(combined)
        }
        Err(_) => {
            // Backend unavailable — return static reference
            ok(format!(
                "Backend unavailable. Static schema reference:\n{NODE_SCHEMA}"
            ))
        }
    }
}
