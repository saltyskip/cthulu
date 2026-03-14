use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use cthulu::api::AppState;
use cthulu::prompts::SavedPrompt;

// ---------------------------------------------------------------------------
// List prompts
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_prompts(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let prompts = state.prompt_repo.list_prompts().await;
    Ok(json!({ "prompts": prompts }))
}

// ---------------------------------------------------------------------------
// Get prompt
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_prompt(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let prompt = state
        .prompt_repo
        .get_prompt(&id)
        .await
        .ok_or_else(|| "prompt not found".to_string())?;

    serde_json::to_value(&prompt).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Create prompt
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreatePromptRequest {
    title: String,
    summary: String,
    #[serde(default)]
    source_flow_name: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[tauri::command]
pub async fn create_prompt(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: CreatePromptRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let prompt = SavedPrompt {
        id: uuid::Uuid::new_v4().to_string(),
        title: request.title,
        summary: request.summary,
        source_flow_name: request.source_flow_name,
        tags: request.tags,
        created_at: chrono::Utc::now(),
    };

    let id = prompt.id.clone();
    state
        .prompt_repo
        .save_prompt(prompt)
        .await
        .map_err(|e| format!("failed to save prompt: {e}"))?;

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Prompt,
        change_type: cthulu::api::changes::ChangeType::Created,
        resource_id: id.clone(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "id": id }))
}

// ---------------------------------------------------------------------------
// Update prompt
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdatePromptRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[tauri::command]
pub async fn update_prompt(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    request: UpdatePromptRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let mut prompt = state
        .prompt_repo
        .get_prompt(&id)
        .await
        .ok_or_else(|| "prompt not found".to_string())?;

    if let Some(title) = request.title {
        prompt.title = title;
    }
    if let Some(summary) = request.summary {
        prompt.summary = summary;
    }
    if let Some(tags) = request.tags {
        prompt.tags = tags;
    }

    state
        .prompt_repo
        .save_prompt(prompt.clone())
        .await
        .map_err(|e| format!("failed to update prompt: {e}"))?;

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Prompt,
        change_type: cthulu::api::changes::ChangeType::Updated,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    serde_json::to_value(&prompt).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Delete prompt
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_prompt(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let existed = state
        .prompt_repo
        .delete_prompt(&id)
        .await
        .map_err(|e| format!("failed to delete prompt: {e}"))?;

    if !existed {
        return Err("prompt not found".to_string());
    }

    let _ = state.changes_tx.send(cthulu::api::changes::ResourceChangeEvent {
        resource_type: cthulu::api::changes::ResourceType::Prompt,
        change_type: cthulu::api::changes::ChangeType::Deleted,
        resource_id: id,
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({ "deleted": true }))
}

// ---------------------------------------------------------------------------
// Summarize session
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SummarizeRequest {
    transcript: String,
    flow_name: String,
    #[serde(default)]
    flow_description: String,
}

#[tauri::command]
pub async fn summarize_session(
    ready: tauri::State<'_, crate::ReadySignal>,
    request: SummarizeRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let meta_prompt = format!(
        r#"You are analyzing a workflow interaction session transcript.
The workflow was called "{}" and described as "{}".

Transcript:
{}

Create a reusable prompt template from this session. Include:
1. A short title (max 60 chars) that captures what this prompt does
2. The distilled prompt that captures the core intent, patterns, and any {{{{variable}}}} placeholders for dynamic content
3. Up to 5 tags that categorize this prompt

Respond ONLY with valid JSON in this exact format:
{{"title": "...", "summary": "...", "tags": ["..."]}}"#,
        request.flow_name, request.flow_description, request.transcript
    );

    let mut child = Command::new("claude")
        .arg("--print")
        .arg("--allowedTools")
        .arg("")
        .arg("-")
        .env_remove("CLAUDECODE")
        .env("CLAUDECODE", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn claude: {e}"))?;

    // Write prompt to stdin
    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(meta_prompt.as_bytes())
            .await
            .map_err(|e| format!("stdin write failed: {e}"))?;
        drop(stdin);
    }

    // Read stdout
    let stdout = child.stdout.take().expect("stdout piped");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut output = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        output.push_str(&line);
        output.push('\n');
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("process wait failed: {e}"))?;

    if !status.success() {
        return Err(format!("claude exited with {status}"));
    }

    // Try to parse the JSON from Claude's output
    let cleaned = output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<Value>(cleaned) {
        Ok(parsed) => {
            let title = parsed
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled Prompt")
                .to_string();
            let summary = parsed
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or(&output)
                .to_string();
            let tags: Vec<String> = parsed
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Ok(json!({
                "title": title,
                "summary": summary,
                "tags": tags,
            }))
        }
        Err(_) => Ok(json!({
            "title": format!("Prompt from {}", request.flow_name),
            "summary": output.trim(),
            "tags": [],
        })),
    }
}
