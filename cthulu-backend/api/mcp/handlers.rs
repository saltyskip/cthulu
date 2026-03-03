//! MCP setup endpoints.
//!
//! GET  /api/mcp/status   — check binary existence, Claude Desktop registration, SearXNG
//! POST /api/mcp/build    — run `cargo build --release --bin cthulu-mcp` (streaming SSE)
//! POST /api/mcp/register — write cthulu entry into Claude Desktop claude_desktop_config.json

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures::stream::Stream;
use serde_json::json;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

use crate::api::AppState;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Compile-time path to the workspace root. `CARGO_MANIFEST_DIR` is set by
/// cargo during build and points to the directory containing `Cargo.toml`.
/// Only valid when running from the original build tree — not portable if the
/// binary is copied to another machine or directory.
const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn project_root() -> &'static Path {
    Path::new(PROJECT_ROOT)
}

fn mcp_binary_path(root: &Path) -> PathBuf {
    root.join("target").join("release").join("cthulu-mcp")
}

fn launcher_path(root: &Path) -> PathBuf {
    root.join("scripts").join("mcp-launcher.sh")
}

/// Claude Desktop config file location.
/// Only meaningful on macOS — returns None on other platforms.
fn claude_config_path() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    })
}

// ── GET /api/mcp/status ───────────────────────────────────────────────────────

pub(crate) async fn mcp_status(State(state): State<AppState>) -> impl IntoResponse {
    let root = project_root();
    let binary = mcp_binary_path(root);
    let launcher = launcher_path(root);
    let config_path = claude_config_path();

    // 1. Binary built?
    let binary_exists = binary.exists();
    let binary_path_str = binary.to_string_lossy().to_string();

    // 2. Launcher exists?
    let launcher_exists = launcher.exists();

    // 3. Registered in Claude Desktop? (async file read to avoid blocking the runtime)
    let (registered, config_entry, config_path_str) = match &config_path {
        Some(path) if path.exists() => {
            match tokio::fs::read_to_string(path).await {
                Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(cfg) => {
                        let entry = cfg
                            .get("mcpServers")
                            .and_then(|s| s.get("cthulu"))
                            .cloned();
                        (entry.is_some(), entry, path.to_string_lossy().to_string())
                    }
                    Err(_) => (false, None, path.to_string_lossy().to_string()),
                },
                Err(_) => (false, None, path.to_string_lossy().to_string()),
            }
        }
        Some(path) => (false, None, path.to_string_lossy().to_string()),
        None => (false, None, "(not supported on this platform)".to_string()),
    };

    // 4. SearXNG reachable? Use shared http_client from AppState.
    let http = state.http_client.clone();
    let searxng_ok = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        async move {
            http.get("http://127.0.0.1:8888/search?q=test&format=json")
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        },
    )
    .await
    .unwrap_or(false);

    tracing::info!(
        binary_exists,
        launcher_exists,
        registered,
        searxng_ok,
        "MCP status check"
    );

    Json(json!({
        "binary_exists": binary_exists,
        "binary_path": binary_path_str,
        "launcher_exists": launcher_exists,
        "registered_in_claude_desktop": registered,
        "config_path": config_path_str,
        "config_entry": config_entry,
        "searxng_ok": searxng_ok,
        "searxng_url": "http://127.0.0.1:8888",
    }))
}

// ── POST /api/mcp/build ───────────────────────────────────────────────────────
// Returns an SSE stream so the UI can show live build output.
// Only one build can run at a time — concurrent requests are rejected.

pub(crate) async fn mcp_build(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let root = project_root().to_path_buf();
    let building = state.mcp_building.clone();

    // Concurrency guard: acquire before entering the stream
    let already_building = building.swap(true, Ordering::SeqCst);

    let stream = async_stream::stream! {
        if already_building {
            tracing::warn!("Rejected concurrent MCP build request");
            yield Ok(Event::default().event("error").data("Build already in progress"));
            yield Ok(Event::default().event("done").data("exit:1"));
            return;
        }

        tracing::info!(?root, "Starting cthulu-mcp release build");
        yield Ok(Event::default().data("[cthulu-mcp] Starting build: cargo build --release --bin cthulu-mcp"));

        let mut child = match Command::new("cargo")
            .args(["build", "--release", "--bin", "cthulu-mcp"])
            .current_dir(&root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to spawn cargo build");
                yield Ok(Event::default().event("error").data(format!("Failed to spawn cargo: {e}")));
                yield Ok(Event::default().event("done").data("exit:1"));
                building.store(false, Ordering::SeqCst);
                return;
            }
        };

        // cargo sends build progress to stderr
        let stderr = child.stderr.take().expect("stderr piped");
        let reader = BufReader::new(stderr);
        let mut lines = LinesStream::new(reader.lines());

        while let Some(line) = lines.next().await {
            match line {
                Ok(text) => yield Ok(Event::default().data(text)),
                Err(e) => {
                    yield Ok(Event::default().event("error").data(format!("Read error: {e}")));
                    break;
                }
            }
        }

        match child.wait().await {
            Ok(status) if status.success() => {
                tracing::info!("cthulu-mcp build succeeded");
                yield Ok(Event::default().data("[cthulu-mcp] Build succeeded."));
                yield Ok(Event::default().event("done").data("exit:0"));
            }
            Ok(status) => {
                let code = status.code().unwrap_or(-1);
                tracing::warn!(code, "cthulu-mcp build failed");
                yield Ok(Event::default().event("error").data(
                    format!("[cthulu-mcp] Build failed with exit code: {code}")
                ));
                yield Ok(Event::default().event("done").data(format!("exit:{}", status.code().unwrap_or(1))));
            }
            Err(e) => {
                tracing::error!(error = %e, "Error waiting for cargo build");
                yield Ok(Event::default().event("error").data(format!("Wait error: {e}")));
                yield Ok(Event::default().event("done").data("exit:1"));
            }
        }

        // Release the build guard
        building.store(false, Ordering::SeqCst);
    };

    Sse::new(stream)
}

// ── POST /api/mcp/register ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct RegisterRequest {
    /// Base URL of the Cthulu backend (default: http://localhost:8081)
    pub base_url: Option<String>,
    /// SearXNG URL (default: http://127.0.0.1:8888)
    pub searxng_url: Option<String>,
}

pub(crate) async fn mcp_register(
    State(_state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let root = project_root();
    let launcher = launcher_path(root);

    let config_path = match claude_config_path() {
        Some(p) => p,
        None => {
            tracing::warn!("MCP register called on unsupported platform");
            return Json(json!({
                "ok": false,
                "error": "Claude Desktop config path is only supported on macOS."
            }));
        }
    };

    let base_url = body.base_url.unwrap_or_else(|| "http://localhost:8081".to_string());
    let searxng_url = body.searxng_url.unwrap_or_else(|| "http://127.0.0.1:8888".to_string());

    // Ensure launcher is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if launcher.exists() {
            let _ = std::fs::set_permissions(
                &launcher,
                std::fs::Permissions::from_mode(0o755),
            );
        }
    }

    // Read or create config
    let config_dir = config_path.parent().unwrap_or(&config_path);
    if let Err(e) = std::fs::create_dir_all(config_dir) {
        tracing::error!(error = %e, ?config_dir, "Cannot create Claude config dir");
        return Json(json!({ "ok": false, "error": format!("Cannot create config dir: {e}") }));
    }

    let mut config: serde_json::Value = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or(json!({})),
            Err(_) => json!({}),
        }
    } else {
        json!({})
    };

    // Merge the cthulu entry
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = json!({});
    }
    config["mcpServers"]["cthulu"] = json!({
        "command": launcher.to_string_lossy(),
        "args": [
            "--base-url", base_url,
            "--searxng-url", searxng_url,
        ]
    });

    // Write atomically via temp file + rename
    let tmp = config_path.with_extension("json.tmp");
    match serde_json::to_string_pretty(&config) {
        Ok(serialized) => {
            if let Err(e) = std::fs::write(&tmp, &serialized) {
                tracing::error!(error = %e, "Failed to write MCP config temp file");
                return Json(json!({ "ok": false, "error": format!("Write failed: {e}") }));
            }
            if let Err(e) = std::fs::rename(&tmp, &config_path) {
                tracing::error!(error = %e, "Failed to rename MCP config temp file");
                return Json(json!({ "ok": false, "error": format!("Rename failed: {e}") }));
            }
            tracing::info!(?config_path, "Registered cthulu MCP server in Claude Desktop");
            Json(json!({
                "ok": true,
                "message": "Registered cthulu MCP server in Claude Desktop config. Restart Claude Desktop to apply.",
                "config_path": config_path.to_string_lossy(),
            }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Serialization failed: {e}") })),
    }
}
