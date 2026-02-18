mod config;
mod github;
mod relay;
mod server;
mod setup;
mod slack_socket;
mod tasks;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::Request;
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::github::client::{GithubClient, HttpGithubClient};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "cthulu", about = "AI automation daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server (default when no subcommand is given)
    Serve,
    /// Generate a Slack app manifest and print setup instructions
    Setup {
        /// Bot display name (prompted interactively if omitted)
        #[arg(long)]
        name: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Handle the setup subcommand before loading config / starting the server
    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Setup { name } => {
            setup::run(name);
            return Ok(());
        }
        Commands::Serve => {
            // Fall through to server startup below
        }
    }

    // -----------------------------------------------------------------------
    // Server startup (existing code, unchanged)
    // -----------------------------------------------------------------------

    dotenv().ok();

    let config = config::Config::load(Path::new("cthulu.toml"))?;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cthulu=info,tower_http=warn,hyper=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(sentry::integrations::tracing::layer().event_filter(
            |metadata| match *metadata.level() {
                tracing::Level::ERROR => sentry::integrations::tracing::EventFilter::Event,
                tracing::Level::WARN | tracing::Level::INFO => {
                    sentry::integrations::tracing::EventFilter::Breadcrumb
                }
                _ => sentry::integrations::tracing::EventFilter::Ignore,
            },
        ))
        .init();

    let _guard = sentry::init((
        config.sentry_dsn(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(config.server.environment.clone().into()),
            send_default_pii: true,
            traces_sample_rate: 0.2,
            enable_logs: true,
            ..Default::default()
        },
    ));

    let http_client = Arc::new(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?,
    );
    let github_token = config.github_token();
    let task_state = Arc::new(tasks::TaskState::new());

    let github_client: Option<Arc<dyn GithubClient>> = github_token.as_ref().map(|token| {
        Arc::new(HttpGithubClient::new((*http_client).clone(), token.clone())) as Arc<dyn GithubClient>
    });

    let config = Arc::new(config);

    // Spawn all configured tasks
    for task in &config.tasks {
        tasks::spawn_task(
            task.clone(),
            github_token.clone(),
            http_client.clone(),
            task_state.clone(),
        )
        .await;
    }

    let app_state = server::AppState {
        task_state,
        config: config.clone(),
        github_client,
        http_client: http_client.clone(),
        bot_user_id: Arc::new(tokio::sync::RwLock::new(None)),
        thread_sessions: relay::new_sessions(),
        seen_event_ids: Arc::new(tokio::sync::RwLock::new(std::collections::VecDeque::new())),
    };

    // Conditionally start Slack Socket Mode relay
    if config.slack.is_some() {
        let state_clone = app_state.clone();
        tokio::spawn(async move {
            relay::resolve_bot_user_id(&state_clone).await;
        });
        let state_clone = app_state.clone();
        tokio::spawn(async move {
            slack_socket::run(state_clone).await;
        });
        tracing::info!("Slack Socket Mode interactive relay enabled");
    }

    let app = server::create_app(app_state)
        .layer(SentryHttpLayer::new().enable_transaction())
        .layer(NewSentryLayer::<Request<Body>>::new_from_top());

    let port = config.server.port;
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
