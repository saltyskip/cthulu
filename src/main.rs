mod config;
mod flows;
mod github;
mod server;
mod tasks;
mod tui;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::Request;
use clap::Parser;
use dotenvy::dotenv;
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::flows::events::RunEvent;
use crate::flows::file_store::FileStore;
use crate::flows::scheduler::FlowScheduler;
use crate::flows::store::Store;
use crate::github::client::{GithubClient, HttpGithubClient};

#[derive(Parser)]
#[command(name = "cthulu", about = "AI-powered flow runner")]
enum Cli {
    /// Start the HTTP server (default when no subcommand is given)
    #[command(alias = "run")]
    Serve {
        /// Start with all flow triggers disabled
        #[arg(long)]
        start_disabled: bool,
    },
    /// Open interactive TUI session
    Tui {
        /// Jump directly to a flow by ID
        #[arg(long)]
        flow: Option<String>,
        /// Server URL to connect to
        #[arg(long, default_value = "http://localhost:8081")]
        server: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Parse CLI args — default to Serve when no subcommand is given,
    // but still allow --help and --version to work.
    let args: Vec<String> = std::env::args().collect();
    let cli = if args.len() <= 1 {
        // No subcommand given, default to serve
        Cli::Serve { start_disabled: false }
    } else {
        Cli::parse()
    };

    match cli {
        Cli::Serve { start_disabled } => run_server(start_disabled).await,
        Cli::Tui { flow, server } => {
            tui::run(server, flow).await?;
            Ok(())
        }
    }
}

async fn run_server(start_disabled: bool) -> Result<(), Box<dyn Error>> {
    let config = config::Config::from_env();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cthulu=info,tower_http=warn,hyper=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_tree::HierarchicalLayer::new(2).with_targets(true).with_bracketed_fields(false))
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
        config.sentry_dsn.clone().unwrap_or_default(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(config.environment.clone().into()),
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

    let github_client: Option<Arc<dyn GithubClient>> = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .map(|token| {
            Arc::new(HttpGithubClient::new((*http_client).clone(), token)) as Arc<dyn GithubClient>
        });

    // Initialize unified store (flows + runs)
    let base_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cthulu");
    let store: Arc<dyn Store> = Arc::new(FileStore::new(base_dir.clone()));
    store
        .load_all()
        .await
        .context("failed to load store")?;


    let (events_tx, _) = tokio::sync::broadcast::channel::<RunEvent>(256);

    // Create and start the flow scheduler
    let scheduler = Arc::new(FlowScheduler::new(
        store.clone(),
        http_client.clone(),
        github_client.clone(),
        events_tx.clone(),
    ));
    if start_disabled {
        tracing::info!("Starting with all flow triggers disabled (--start-disabled)");
        let flows = store.list_flows().await;
        for mut flow in flows {
            if flow.enabled {
                flow.enabled = false;
                if let Err(e) = store.save_flow(flow).await {
                    tracing::warn!(error = %e, "Failed to disable flow");
                }
            }
        }
    } else {
        scheduler.start_all().await;
    }

    // Load persisted interact sessions from sessions.yaml in the current directory
    let sessions_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("sessions.yaml");
    let persisted_sessions = server::load_sessions(&sessions_path);

    let app_state = server::AppState {
        github_client,
        http_client,
        store,
        scheduler,
        events_tx,
        interact_sessions: Arc::new(tokio::sync::RwLock::new(persisted_sessions)),
        sessions_path,
        data_dir: base_dir,
        live_processes: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    let app = server::create_app(app_state)
        .layer(SentryHttpLayer::new().enable_transaction())
        .layer(NewSentryLayer::<Request<Body>>::new_from_top());

    let port = config.port;
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
