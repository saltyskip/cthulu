use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

use super::middleware;
use super::AppState;
use crate::tasks::diff;
use crate::tasks::executors::Executor;

pub fn build_router(state: AppState) -> Router {
    let health_routes = Router::new().route(
        "/",
        get(|| async {
            Json(json!({
                "status": "ok",
            }))
        }),
    );

    Router::new()
        .nest("/health", health_routes)
        .route("/claude", post(run_claude))
        .route("/reviews/status", get(review_status))
        .route("/reviews/trigger", post(trigger_review))
        .fallback(not_found)
        .with_state(state)
        .layer(axum::middleware::from_fn(middleware::strip_trailing_slash))
        .layer(axum::middleware::from_fn(
            middleware::enrich_current_span_middleware,
        ))
}

async fn not_found(req: axum::extract::Request) -> impl IntoResponse {
    tracing::warn!("unhandled path: {}", req.uri());
    (StatusCode::NOT_FOUND, "Not Found")
}

// --- Claude proxy ---

#[derive(Deserialize)]
pub struct ClaudeRequest {
    pub prompt: String,
    pub working_dir: Option<String>,
}

#[tracing::instrument(skip_all, fields(prompt))]
pub async fn run_claude(
    Json(body): Json<ClaudeRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!(prompt = %body.prompt, "spawning claude process");

    let working_dir = body.working_dir.unwrap_or_else(|| ".".to_string());

    let stream = async_stream::stream! {
        let mut child = match Command::new("claude")
            .arg("--print")
            .arg("--dangerously-skip-permissions")
            .arg(&body.prompt)
            .current_dir(&working_dir)
            .env_remove("CLAUDECODE")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn claude process");
                yield Ok(Event::default().data(format!("error: failed to spawn claude: {e}")));
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let reader = BufReader::new(stdout);
        let mut lines = LinesStream::new(reader.lines());

        while let Some(line) = lines.next().await {
            match line {
                Ok(text) => {
                    yield Ok(Event::default().data(text));
                }
                Err(e) => {
                    tracing::error!(error = %e, "error reading claude output");
                    yield Ok(Event::default().data(format!("error: {e}")));
                    break;
                }
            }
        }

        let err_reader = BufReader::new(stderr);
        let mut err_lines = LinesStream::new(err_reader.lines());
        while let Some(line) = err_lines.next().await {
            if let Ok(text) = line {
                if !text.is_empty() {
                    yield Ok(Event::default().event("stderr").data(text));
                }
            }
        }

        match child.wait().await {
            Ok(status) => {
                yield Ok(Event::default().event("done").data(format!("exit: {status}")));
            }
            Err(e) => {
                yield Ok(Event::default().event("done").data(format!("error waiting: {e}")));
            }
        }
    };

    Sse::new(stream)
}

// --- Review status & trigger ---

async fn review_status(State(state): State<AppState>) -> Json<Value> {
    let task_state = &state.task_state;

    let seen = task_state.seen_prs.lock().await;
    let completed = *task_state.reviews_completed.lock().await;
    let active = *task_state.active_reviews.lock().await;

    let seen_prs: serde_json::Map<String, Value> = seen
        .iter()
        .map(|(repo, prs)| {
            let pr_map: serde_json::Map<String, Value> = prs
                .iter()
                .map(|(num, sha)| (num.to_string(), json!(sha)))
                .collect();
            (repo.clone(), json!(pr_map))
        })
        .collect();

    Json(json!({
        "reviews_completed": completed,
        "active_reviews": active,
        "seen_prs": seen_prs,
    }))
}

#[derive(Deserialize)]
struct TriggerRequest {
    repo: String,
    pr: u64,
}

async fn trigger_review(
    State(state): State<AppState>,
    Json(body): Json<TriggerRequest>,
) -> (StatusCode, Json<Value>) {
    let config = &state.config;

    let matching_task = config.tasks.iter().find(|t| {
        t.trigger
            .github
            .as_ref()
            .is_some_and(|gh| gh.repos.iter().any(|r| r.slug == body.repo))
    });

    let Some(task) = matching_task else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("no task configured for repo '{}'", body.repo) })),
        );
    };

    let Some(gh_trigger) = &task.trigger.github else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "task has no github trigger" })),
        );
    };

    let repo_entry = gh_trigger
        .repos
        .iter()
        .find(|r| r.slug == body.repo)
        .unwrap();

    let Some(github_client) = &state.github_client else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "GITHUB_TOKEN not configured" })),
        );
    };

    let prompt_template = match std::fs::read_to_string(&task.prompt) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to read prompt: {e}") })),
            );
        }
    };

    let (owner, repo_name) = repo_entry.owner_repo().unwrap();
    let pr_number = body.pr;
    let task_state = state.task_state.clone();
    let permissions = task.permissions.clone();
    let local_path = repo_entry.path.clone();
    let owner = owner.to_string();
    let repo_name = repo_name.to_string();
    let repo_slug = body.repo.clone();
    let max_diff_size = gh_trigger.max_diff_size;
    let github_client = github_client.clone();

    tokio::spawn(async move {
        let pr = match github_client
            .fetch_single_pr(&owner, &repo_name, pr_number)
            .await
        {
            Ok(pr) => pr,
            Err(e) => {
                tracing::error!(error = %e, "Failed to fetch PR for manual trigger");
                return;
            }
        };

        // Mark as seen with actual SHA (after fetch to avoid race with poll loop)
        {
            let mut seen = task_state.seen_prs.lock().await;
            seen.entry(repo_slug.clone())
                .or_default()
                .insert(pr_number, pr.head.sha.clone());
        }

        let diff = match github_client
            .fetch_pr_diff(&owner, &repo_name, pr_number)
            .await
        {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "Failed to fetch diff for manual trigger");
                return;
            }
        };

        let diff_ctx = match diff::prepare_diff_context(&diff, pr_number, max_diff_size) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!(error = %e, "Failed to prepare diff context for manual trigger");
                return;
            }
        };
        let mut context = std::collections::HashMap::new();
        context.insert("diff".to_string(), diff_ctx.text());
        context.insert("pr_number".to_string(), pr.number.to_string());
        context.insert("pr_title".to_string(), pr.title.clone());
        context.insert("pr_body".to_string(), pr.body.unwrap_or_default());
        context.insert("base_ref".to_string(), pr.base.ref_name.clone());
        context.insert("head_ref".to_string(), pr.head.ref_name.clone());
        context.insert("head_sha".to_string(), pr.head.sha.clone());
        context.insert("repo".to_string(), repo_slug);
        context.insert("local_path".to_string(), local_path.display().to_string());
        context.insert("review_type".to_string(), "initial".to_string());

        let rendered = crate::tasks::context::render_prompt(&prompt_template, &context);

        let executor =
            crate::tasks::executors::claude_code::ClaudeCodeExecutor::new(permissions);

        {
            let mut active = task_state.active_reviews.lock().await;
            *active += 1;
        }

        let result = executor.execute(&rendered, &local_path).await;

        {
            let mut active = task_state.active_reviews.lock().await;
            *active -= 1;
        }

        match result {
            Ok(exec_result) => {
                let mut completed = task_state.reviews_completed.lock().await;
                *completed += 1;
                tracing::info!(
                    pr = pr_number,
                    cost_usd = exec_result.cost_usd,
                    turns = exec_result.num_turns,
                    "Manual review completed ({} turns, ${:.4})",
                    exec_result.num_turns,
                    exec_result.cost_usd
                );
            }
            Err(e) => {
                tracing::error!(pr = pr_number, error = %e, "Manual review failed");
            }
        }

        diff::cleanup(&diff_ctx);
    });

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "review_started",
            "repo": body.repo,
            "pr": body.pr,
        })),
    )
}
