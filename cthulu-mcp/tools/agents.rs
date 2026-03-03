use rmcp::{model::CallToolResult, ErrorData as McpError};
use serde_json::Value;

use super::{err, ok, CthuluMcpServer};

pub async fn list_agents(s: &CthuluMcpServer) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.list_agents().await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn get_agent(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.get_agent(&id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn create_agent(s: &CthuluMcpServer, body: String) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.create_agent(parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn update_agent(
    s: &CthuluMcpServer,
    id: String,
    body: String,
) -> Result<CallToolResult, McpError> {
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| err(format!("invalid JSON: {e}")))?;
    let v = s.cthulu.update_agent(&id, parsed).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn delete_agent(s: &CthuluMcpServer, id: String) -> Result<CallToolResult, McpError> {
    s.cthulu.delete_agent(&id).await.map_err(err)?;
    ok(format!("Agent {id} deleted."))
}

pub async fn list_agent_sessions(
    s: &CthuluMcpServer,
    agent_id: String,
) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.list_agent_sessions(&agent_id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn create_agent_session(
    s: &CthuluMcpServer,
    agent_id: String,
) -> Result<CallToolResult, McpError> {
    let v = s.cthulu.create_agent_session(&agent_id).await.map_err(err)?;
    ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

pub async fn delete_agent_session(
    s: &CthuluMcpServer,
    agent_id: String,
    session_id: String,
) -> Result<CallToolResult, McpError> {
    s.cthulu
        .delete_agent_session(&agent_id, &session_id)
        .await
        .map_err(err)?;
    ok(format!("Session {session_id} deleted."))
}

pub async fn get_session_log(
    s: &CthuluMcpServer,
    agent_id: String,
    session_id: String,
) -> Result<CallToolResult, McpError> {
    let v = s
        .cthulu
        .get_session_log(&agent_id, &session_id)
        .await
        .map_err(err)?;
    ok(v.as_str().unwrap_or("(empty log)"))
}

pub async fn chat_with_agent(
    s: &CthuluMcpServer,
    agent_id: String,
    message: String,
    session_id: Option<String>,
) -> Result<CallToolResult, McpError> {
    // Create a session if none provided
    let sid = if let Some(id) = session_id {
        id
    } else {
        let session = s
            .cthulu
            .create_agent_session(&agent_id)
            .await
            .map_err(err)?;
        session
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("created session has no session_id field"))?
            .to_string()
    };

    let reply = s
        .cthulu
        .chat_with_agent(&agent_id, &sid, &message)
        .await
        .map_err(err)?;

    ok(format!("session_id: {sid}\n\n{reply}"))
}
