//! cthulu-mcp — MCP server for the Cthulu AI workflow automation platform.
//!
//! Exposes 30 tools over stdio (JSON-RPC 2.0 / MCP protocol):
//!   - 2  web search tools  (DuckDuckGo via SearXNG or direct scrape fallback)
//!   - 8  flow management tools
//!   - 10 agent management tools  (incl. chat_with_agent with polling)
//!   - 5  prompt library tools
//!   - 5  utility tools (templates, cron, scheduler, token)
//!
//! Usage:
//!   cthulu-mcp [--base-url http://localhost:8081] [--searxng-url http://localhost:8888]
//!
//! Claude Desktop config:
//!   {
//!     "mcpServers": {
//!       "cthulu": {
//!         "command": "/path/to/target/release/cthulu-mcp",
//!         "args": ["--base-url", "http://localhost:8081",
//!                  "--searxng-url", "http://localhost:8888"]
//!       }
//!     }
//!   }

mod client;
mod rate_limiter;
mod search;
mod tools;

use std::sync::Arc;

use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{EnvFilter, fmt};

use client::CthuluClient;
use search::SearchClient;
use tools::CthuluMcpServer;

// ── CLI args ──────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "cthulu-mcp",
    about = "MCP server for the Cthulu AI workflow automation platform",
    version
)]
struct Args {
    /// Base URL of the Cthulu backend
    #[arg(long, default_value = "http://localhost:8081")]
    base_url: String,

    /// Base URL of the self-hosted SearXNG instance.
    /// Set to "disabled" to skip SearXNG and always use the DuckDuckGo fallback.
    #[arg(long, default_value = "http://localhost:8888")]
    searxng_url: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging goes to stderr so it doesn't pollute the MCP stdio transport
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    let searxng = if args.searxng_url.eq_ignore_ascii_case("disabled") {
        None
    } else {
        Some(args.searxng_url.clone())
    };

    eprintln!(
        "[cthulu-mcp] starting — backend: {} | searxng: {}",
        args.base_url,
        searxng.as_deref().unwrap_or("disabled (DDG fallback)")
    );

    let cthulu = Arc::new(CthuluClient::new(&args.base_url));
    let search = Arc::new(SearchClient::new(searxng));
    let server = CthuluMcpServer::new(cthulu, search);

    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| eprintln!("[cthulu-mcp] server error: {e}"))?;

    service.waiting().await?;
    Ok(())
}
