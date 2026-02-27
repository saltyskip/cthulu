use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use hyper::StatusCode;
use hyper::header;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};

use super::middleware;
use super::AppState;

pub fn build_router(state: AppState) -> Router {
    let health_routes = Router::new().route(
        "/",
        get(|| async {
            Json(json!({
                "status": "ok",
            }))
        }),
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(vec![header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        .nest("/health", health_routes)
        .route("/claude", post(run_claude))
        .nest("/api", api_router())
        .fallback(not_found)
        .with_state(state)
        .layer(cors)
        .layer(axum::middleware::from_fn(middleware::strip_trailing_slash))
        .layer(axum::middleware::from_fn(
            middleware::enrich_current_span_middleware,
        ))
}

fn api_router() -> Router<AppState> {
    Router::new()
        .merge(super::flows::router())
        .merge(super::agents::router())
        .merge(super::prompts::router())
        .merge(super::templates::router())
        .merge(super::auth::router())
        .merge(super::scheduler::router())
        .merge(super::sandbox::router())
        .merge(super::changes::router())
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
