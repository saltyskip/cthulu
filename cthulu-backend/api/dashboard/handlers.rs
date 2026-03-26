/// Dashboard endpoints — real-world task runner.
///
/// GET  /api/dashboard/tasks     — list saved task templates
/// POST /api/dashboard/tasks     — save a new task template
/// POST /api/dashboard/run       — run a task (sends to an agent via Claude CLI)
/// GET  /api/dashboard/history   — recent task run history
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Stdio;
use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::api::AppState;

/// Timeout for Claude CLI task execution.
const TASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Maximum concurrent task runs.
static TASK_SEMAPHORE: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(2));

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A saved task template (e.g. "Add groceries to Instacart").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The prompt sent to the agent.
    pub prompt: String,
    /// Category for UI grouping (e.g. "shopping", "research", "data-entry").
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub created_at: String,
}

/// A completed task run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub id: String,
    pub task_name: String,
    pub prompt: String,
    pub status: String, // "running" | "completed" | "failed"
    pub result: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Config file: saved task templates + run history.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DashboardData {
    #[serde(default)]
    tasks: Vec<TaskTemplate>,
    #[serde(default)]
    history: Vec<TaskRun>,
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

fn data_path(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("dashboard.json")
}

fn read_data(state: &AppState) -> DashboardData {
    let path = data_path(state);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => {
            // Seed with example tasks on first run
            let mut data = DashboardData::default();
            data.tasks = default_task_templates();
            data
        }
    }
}

fn write_data(state: &AppState, data: &DashboardData) {
    let path = data_path(state);
    let tmp_path = path.with_extension("json.tmp");
    if let Ok(json_str) = serde_json::to_string_pretty(data) {
        let _ = std::fs::write(&tmp_path, &json_str);
        let _ = std::fs::rename(&tmp_path, &path);
    }
}

/// Built-in example task templates seeded on first run.
fn default_task_templates() -> Vec<TaskTemplate> {
    vec![
        TaskTemplate {
            id: "grocery-list".to_string(),
            name: "Add Groceries to Cart".to_string(),
            description: "Search for grocery items and add them to an online shopping cart.".to_string(),
            prompt: r#"You are a helpful shopping assistant. The user wants to add groceries to their online cart.

Here is their grocery list:
- Milk (1 gallon, whole)
- Eggs (1 dozen, large)
- Bread (whole wheat loaf)
- Bananas (1 bunch)
- Chicken breast (2 lbs)
- Rice (2 lb bag, jasmine)

For each item, search for it, find the best match, and report:
1. Item name and brand found
2. Price
3. Availability (in stock / out of stock)

Output a summary table at the end with total estimated cost."#.to_string(),
            category: "shopping".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
        TaskTemplate {
            id: "price-compare".to_string(),
            name: "Compare Product Prices".to_string(),
            description: "Compare prices for a product across multiple sources.".to_string(),
            prompt: r#"Compare prices for "Sony WH-1000XM5 headphones" across at least 3 online retailers.

For each retailer, find:
1. Current price
2. Whether it's on sale
3. Shipping cost / free shipping
4. Estimated delivery time

Present results in a comparison table sorted by total cost (price + shipping)."#.to_string(),
            category: "shopping".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
        TaskTemplate {
            id: "research-summary".to_string(),
            name: "Research Summary".to_string(),
            description: "Research a topic and produce a structured summary.".to_string(),
            prompt: r#"Research the current state of home battery storage systems (e.g., Tesla Powerwall, Enphase, LG RESU).

Produce a brief report covering:
1. Top 3 products with specs (capacity, power output, warranty)
2. Price ranges
3. Pros and cons of each
4. Best option for a typical 2,000 sq ft home with solar panels

Keep it under 500 words."#.to_string(),
            category: "research".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/dashboard/tasks — list saved task templates
pub(crate) async fn list_tasks(State(state): State<AppState>) -> impl IntoResponse {
    let data = read_data(&state);
    Json(json!({ "tasks": data.tasks }))
}

/// POST /api/dashboard/tasks — save a new task template
pub(crate) async fn save_task(
    State(state): State<AppState>,
    Json(mut task): Json<TaskTemplate>,
) -> impl IntoResponse {
    if task.id.is_empty() {
        task.id = uuid::Uuid::new_v4().to_string();
    }
    if task.created_at.is_empty() {
        task.created_at = chrono::Utc::now().to_rfc3339();
    }

    let mut data = read_data(&state);
    // Upsert: replace if exists, else push
    if let Some(existing) = data.tasks.iter_mut().find(|t| t.id == task.id) {
        *existing = task.clone();
    } else {
        data.tasks.push(task.clone());
    }
    write_data(&state, &data);

    (StatusCode::OK, Json(json!({ "task": task })))
}

/// DELETE /api/dashboard/tasks/{id}
pub(crate) async fn delete_task(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut data = read_data(&state);
    let before = data.tasks.len();
    data.tasks.retain(|t| t.id != id);
    let deleted = data.tasks.len() < before;
    write_data(&state, &data);
    Json(json!({ "deleted": deleted }))
}

/// POST /api/dashboard/run — execute a task via Claude CLI
#[derive(Deserialize)]
pub(crate) struct RunTaskRequest {
    /// The prompt to send to Claude.
    pub prompt: String,
    /// Display name for history.
    #[serde(default)]
    pub task_name: String,
}

pub(crate) async fn run_task(
    State(state): State<AppState>,
    Json(body): Json<RunTaskRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let _permit = TASK_SEMAPHORE.acquire().await.map_err(|_| {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "Task runner unavailable" })))
    })?;

    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();

    // Record the run as "running" in history
    {
        let mut data = read_data(&state);
        data.history.insert(0, TaskRun {
            id: run_id.clone(),
            task_name: body.task_name.clone(),
            prompt: body.prompt.clone(),
            status: "running".to_string(),
            result: None,
            error: None,
            started_at: started_at.clone(),
            finished_at: None,
        });
        // Keep last 50 runs
        data.history.truncate(50);
        write_data(&state, &data);
    }

    // Spawn Claude CLI to execute the task
    let mut child = Command::new("claude")
        .arg("--print")
        .arg("--allowedTools")
        .arg("") // no tools — pure text completion
        .arg("-")
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            tracing::error!(error = %e, "failed to spawn claude for task run");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("failed to spawn claude: {e}") })))
        })?;

    let claude_result = tokio::time::timeout(TASK_TIMEOUT, async {
        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            stdin.write_all(body.prompt.as_bytes()).await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("stdin write failed: {e}") })))
            })?;
            drop(stdin);
        }

        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut output = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push_str(&line);
            output.push('\n');
        }

        let status = child.wait().await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("process wait failed: {e}") })))
        })?;

        if !status.success() {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                "error": format!("claude exited with {status}")
            }))));
        }

        Ok(output)
    })
    .await;

    let finished_at = chrono::Utc::now().to_rfc3339();

    let (result_text, run_status, error_text) = match claude_result {
        Ok(Ok(output)) => (Some(output.trim().to_string()), "completed", None),
        Ok(Err((_, err_json))) => {
            let err_msg = err_json.0.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error").to_string();
            (None, "failed", Some(err_msg))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            (None, "failed", Some(format!("Task timed out after {}s", TASK_TIMEOUT.as_secs())))
        }
    };

    // Update run history
    {
        let mut data = read_data(&state);
        if let Some(run) = data.history.iter_mut().find(|r| r.id == run_id) {
            run.status = run_status.to_string();
            run.result = result_text.clone();
            run.error = error_text.clone();
            run.finished_at = Some(finished_at.clone());
        }
        write_data(&state, &data);
    }

    Ok((StatusCode::OK, Json(json!({
        "run_id": run_id,
        "status": run_status,
        "result": result_text,
        "error": error_text,
        "started_at": started_at,
        "finished_at": finished_at,
    }))))
}

/// GET /api/dashboard/history — recent task run history
pub(crate) async fn get_history(State(state): State<AppState>) -> impl IntoResponse {
    let data = read_data(&state);
    Json(json!({ "history": data.history }))
}

// ---------------------------------------------------------------------------
// Repo Todo Extractor
// ---------------------------------------------------------------------------

/// POST /api/dashboard/extract-todos — fetch markdown files from a GitHub repo path
/// and extract a todo list from them.
#[derive(Deserialize)]
pub(crate) struct ExtractTodosRequest {
    /// GitHub repo in "owner/repo" format (e.g. "bitcoin-portal/web-monorepo").
    pub repo: String,
    /// Path within the repo (e.g. "docs/daily" or "notes/2026-03-25").
    pub path: String,
    /// Optional branch (defaults to main).
    #[serde(default)]
    pub branch: Option<String>,
}

pub(crate) async fn extract_todos(
    State(state): State<AppState>,
    Json(body): Json<ExtractTodosRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let _permit = TASK_SEMAPHORE.acquire().await.map_err(|_| {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "Task runner unavailable" })))
    })?;

    let branch = body.branch.as_deref().unwrap_or("main");

    // 1. Fetch file listing from GitHub API
    let gh_url = format!(
        "https://api.github.com/repos/{}/contents/{}?ref={}",
        body.repo, body.path, branch
    );

    let gh_token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.is_empty() && t.len() > 10);

    let build_gh_request = |url: &str| {
        let mut req = state.http_client.get(url)
            .header("User-Agent", "cthulu-dashboard")
            .header("Accept", "application/vnd.github.v3+json");
        if let Some(ref token) = gh_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        req
    };

    let gh_resp = build_gh_request(&gh_url).send().await.map_err(|e| {
        (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("GitHub API error: {e}") })))
    })?;

    // If token auth failed, retry without token (public repos)
    let gh_resp = if gh_resp.status() == reqwest::StatusCode::UNAUTHORIZED && gh_token.is_some() {
        tracing::warn!("GitHub token rejected, retrying without auth (public repo fallback)");
        state.http_client.get(&gh_url)
            .header("User-Agent", "cthulu-dashboard")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| {
                (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("GitHub API error: {e}") })))
            })?
    } else {
        gh_resp
    };

    if !gh_resp.status().is_success() {
        let status = gh_resp.status();
        let body_text = gh_resp.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, Json(json!({
            "error": format!("GitHub returned {status}: {body_text}")
        }))));
    }

    let files: serde_json::Value = gh_resp.json().await.map_err(|e| {
        (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Failed to parse GitHub response: {e}") })))
    })?;

    // 2. Filter to .md files and fetch their content
    let md_files: Vec<&serde_json::Value> = match files.as_array() {
        Some(arr) => arr.iter().filter(|f| {
            f["name"].as_str().map_or(false, |n| n.ends_with(".md"))
        }).collect(),
        None => {
            // Single file response (not a directory)
            if files["name"].as_str().map_or(false, |n| n.ends_with(".md")) {
                vec![&files]
            } else {
                return Err((StatusCode::BAD_REQUEST, Json(json!({
                    "error": "Path does not contain markdown files"
                }))));
            }
        }
    };

    if md_files.is_empty() {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "error": format!("No .md files found in {}/{}", body.repo, body.path)
        }))));
    }

    let mut all_content = String::new();
    let mut file_names: Vec<String> = Vec::new();

    for file in &md_files {
        let download_url = file["download_url"].as_str().unwrap_or("");
        let name = file["name"].as_str().unwrap_or("unknown.md");
        if download_url.is_empty() { continue; }

        match state.http_client.get(download_url)
            .header("User-Agent", "cthulu-dashboard")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    all_content.push_str(&format!("\n\n--- {} ---\n\n{}", name, text));
                    file_names.push(name.to_string());
                }
            }
            _ => {
                tracing::warn!(file = %name, "failed to fetch markdown file");
            }
        }
    }

    if all_content.is_empty() {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "error": "Could not fetch any markdown file content"
        }))));
    }

    // 3. Send to Claude to extract todos
    let prompt = format!(
        r#"Below are markdown files from the repo "{repo}" at path "{path}".

Extract ALL actionable todo items, tasks, action items, and things that need to be done.

For each todo, include:
- A clear, actionable description
- Priority (high/medium/low) if inferable from context
- Source file name

Output as a clean markdown checklist:

```
- [ ] [high] Description (from filename.md)
- [ ] [medium] Description (from filename.md)
```

If there are no todos found, say "No action items found."

---

{content}"#,
        repo = body.repo,
        path = body.path,
        content = all_content,
    );

    // Record in history
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();
    {
        let mut data = read_data(&state);
        data.history.insert(0, TaskRun {
            id: run_id.clone(),
            task_name: format!("Todos from {}/{}", body.repo, body.path),
            prompt: format!("Extract todos from {} markdown files", file_names.len()),
            status: "running".to_string(),
            result: None,
            error: None,
            started_at: started_at.clone(),
            finished_at: None,
        });
        data.history.truncate(50);
        write_data(&state, &data);
    }

    let mut child = Command::new("claude")
        .arg("--print")
        .arg("--allowedTools").arg("")
        .arg("-")
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("failed to spawn claude: {e}") })))
        })?;

    let claude_result = tokio::time::timeout(TASK_TIMEOUT, async {
        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("stdin write failed: {e}") })))
            })?;
            drop(stdin);
        }

        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut output = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push_str(&line);
            output.push('\n');
        }
        let status = child.wait().await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("process wait failed: {e}") })))
        })?;
        if !status.success() {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("claude exited with {status}") }))));
        }
        Ok(output)
    }).await;

    let finished_at = chrono::Utc::now().to_rfc3339();

    let (result_text, run_status, error_text) = match claude_result {
        Ok(Ok(output)) => (Some(output.trim().to_string()), "completed", None),
        Ok(Err((_, err_json))) => {
            let err_msg = err_json.0.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error").to_string();
            (None, "failed", Some(err_msg))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            (None, "failed", Some(format!("Timed out after {}s", TASK_TIMEOUT.as_secs())))
        }
    };

    // Update history
    {
        let mut data = read_data(&state);
        if let Some(run) = data.history.iter_mut().find(|r| r.id == run_id) {
            run.status = run_status.to_string();
            run.result = result_text.clone();
            run.error = error_text.clone();
            run.finished_at = Some(finished_at.clone());
        }
        write_data(&state, &data);
    }

    Ok((StatusCode::OK, Json(json!({
        "run_id": run_id,
        "status": run_status,
        "files": file_names,
        "todos": result_text,
        "error": error_text,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_templates_are_valid() {
        let templates = default_task_templates();
        assert!(templates.len() >= 2);
        for t in &templates {
            assert!(!t.id.is_empty());
            assert!(!t.name.is_empty());
            assert!(!t.prompt.is_empty());
        }
    }

    #[test]
    fn dashboard_data_roundtrips() {
        let mut data = DashboardData::default();
        data.tasks = default_task_templates();
        let json = serde_json::to_string(&data).unwrap();
        let parsed: DashboardData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tasks.len(), data.tasks.len());
    }

    #[test]
    fn task_run_serializes() {
        let run = TaskRun {
            id: "r1".into(),
            task_name: "Test".into(),
            prompt: "Do something".into(),
            status: "completed".into(),
            result: Some("Done".into()),
            error: None,
            started_at: "2024-01-01T00:00:00Z".into(),
            finished_at: Some("2024-01-01T00:01:00Z".into()),
        };
        let json = serde_json::to_string(&run).unwrap();
        assert!(json.contains("completed"));
    }

    #[test]
    fn task_timeout_is_reasonable() {
        assert!(TASK_TIMEOUT.as_secs() >= 60);
        assert!(TASK_TIMEOUT.as_secs() <= 600);
    }
}
