use rmcp::{model::CallToolResult, ErrorData as McpError};
use serde_json::Value;

use super::{err, ok, CthuluMcpServer};

pub async fn list_prompts(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.list_prompts().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_prompt(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_prompt(&id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn create_prompt(
    s: &CthuluMcpServer,
    body: String,
) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.create_prompt(parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn update_prompt(
    s: &CthuluMcpServer,
    id: String,
    body: String,
) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.update_prompt(&id, parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn delete_prompt(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    s.cthulu.delete_prompt(&id).await.map_err(err)?;
    ok(format!("Prompt {id} deleted."))
}
