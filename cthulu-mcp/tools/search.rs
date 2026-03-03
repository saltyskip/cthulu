use rmcp::{model::CallToolResult, ErrorData as McpError};

use super::{err, ok, CthuluMcpServer};

pub async fn web_search(
    s: &CthuluMcpServer,
    query: String,
    max_results: Option<u32>,
) -> Result<CallToolResult, McpError> {
    let n = max_results.unwrap_or(10) as usize;
    let result = s.search.search(&query, n).await.map_err(err)?;
    ok(result)
}

pub async fn fetch_content(
    s: &CthuluMcpServer,
    url: String,
) -> Result<CallToolResult, McpError> {
    let result = s.search.fetch_content(&url).await.map_err(err)?;
    ok(result)
}
