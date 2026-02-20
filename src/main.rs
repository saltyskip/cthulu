mod config;
mod flows;
mod github;
mod server;
mod tasks;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::Request;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

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
    let store: Arc<dyn Store> = Arc::new(FileStore::new(base_dir));
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
    scheduler.start_all().await;

    let app_state = server::AppState {
        github_client,
        http_client,
        store,
        scheduler,
        events_tx,
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
