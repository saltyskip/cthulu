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
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::flows::history::RunHistory;
use crate::flows::storage::FlowStore;
use crate::github::client::{GithubClient, HttpGithubClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

    // Initialize flow store
    let flows_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cthulu")
        .join("flows");
    let flow_store = Arc::new(FlowStore::new(flows_dir));
    flow_store
        .load_all()
        .await
        .context("failed to load flows")?;

    // Import TOML tasks as flows on first boot
    if flow_store.is_empty().await && !config.tasks.is_empty() {
        tracing::info!("No flows found, importing {} TOML tasks", config.tasks.len());
        match flows::import::import_toml_tasks(&config.tasks, &flow_store).await {
            Ok(count) => tracing::info!(count, "TOML tasks imported as flows"),
            Err(e) => tracing::error!(error = %e, "Failed to import TOML tasks"),
        }
    }

    let run_history = Arc::new(RunHistory::new());

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
        http_client,
        flow_store,
        run_history,
    };

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
