use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use super::AppState;
use crate::flows::SavedPrompt;

pub fn prompt_router() -> Router<AppState> {
    Router::new()
        .route("/prompts", get(list_prompts).post(create_prompt))
        .route(
            "/prompts/{id}",
            get(get_prompt).put(update_prompt).delete(delete_prompt),
        )
        .route("/prompts/summarize", post(summarize_session))
}

async fn list_prompts(State(state): State<AppState>) -> Json<Value> {
    let prompts = state.store.list_prompts().await;
    Json(json!({ "prompts": prompts }))
}

async fn get_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prompt = state.store.get_prompt(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "prompt not found" })),
        )
    })?;
    Ok(Json(serde_json::to_value(&prompt).unwrap()))
}

#[derive(Deserialize)]
struct CreatePromptRequest {
    title: String,
    summary: String,
    #[serde(default)]
    source_flow_name: String,
    #[serde(default)]
    tags: Vec<String>,
}

async fn create_prompt(
    State(state): State<AppState>,
    Json(body): Json<CreatePromptRequest>,
) -> (StatusCode, Json<Value>) {
    let prompt = SavedPrompt {
        id: Uuid::new_v4().to_string(),
        title: body.title,
        summary: body.summary,
        source_flow_name: body.source_flow_name,
        tags: body.tags,
        created_at: Utc::now(),
    };

    let id = prompt.id.clone();
    if let Err(e) = state.store.save_prompt(prompt).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save prompt: {e}") })),
        );
    }

    (StatusCode::CREATED, Json(json!({ "id": id })))
}

async fn delete_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let existed = state.store.delete_prompt(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to delete prompt: {e}") })),
        )
    })?;

    if !existed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "prompt not found" })),
        ));
    }

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct UpdatePromptRequest {
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

async fn update_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdatePromptRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut prompt = state.store.get_prompt(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "prompt not found" })),
        )
    })?;

    if let Some(title) = body.title {
        prompt.title = title;
    }
    if let Some(summary) = body.summary {
        prompt.summary = summary;
    }
    if let Some(tags) = body.tags {
        prompt.tags = tags;
    }

    state.store.save_prompt(prompt.clone()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to update prompt: {e}") })),
        )
    })?;

    Ok(Json(serde_json::to_value(&prompt).unwrap()))
}

#[derive(Deserialize)]
struct SummarizeRequest {
    transcript: String,
    flow_name: String,
    #[serde(default)]
    flow_description: String,
}

async fn summarize_session(
    Json(body): Json<SummarizeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
        body.flow_name, body.flow_description, body.transcript
    );

    // Use --allowedTools with no tools to prevent arbitrary tool execution
    // (the summarize endpoint is HTTP-reachable and the transcript is user-controlled).
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
        .map_err(|e| {
            tracing::error!(error = %e, "failed to spawn claude for summarize");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to spawn claude: {e}") })),
            )
        })?;

    // Write prompt to stdin
    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin.write_all(meta_prompt.as_bytes()).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("stdin write failed: {e}") })),
            )
        })?;
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

    let status = child.wait().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("process wait failed: {e}") })),
        )
    })?;

    if !status.success() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("claude exited with {status}") })),
        ));
    }

    // Try to parse the JSON from Claude's output
    // Claude might wrap it in markdown code blocks, so strip those
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

            Ok(Json(json!({
                "title": title,
                "summary": summary,
                "tags": tags,
            })))
        }
        Err(_) => {
            // If Claude didn't return valid JSON, return the raw text as summary
            Ok(Json(json!({
                "title": format!("Prompt from {}", body.flow_name),
                "summary": output.trim(),
                "tags": [],
            })))
        }
    }
}
