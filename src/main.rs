mod config;
mod flows;
mod github;
mod sandbox;
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

    // Load persisted interact sessions from ~/.cthulu/sessions.yaml (alongside other state)
    let sessions_path = base_dir.join("sessions.yaml");
    let persisted_sessions = server::load_sessions(&sessions_path);

    // Initialize sandbox provider (before scheduler, so scheduler can use it)
    //
    // Priority:
    //   1. FIRECRACKER_SSH_HOST → RemoteSsh (real Linux server with /dev/kvm)
    //   2. FIRECRACKER_API_URL → LimaTcp (Lima VM on macOS, FC API over TCP)
    //   3. Default → DangerousHost (best-effort host isolation, no VM)
    let sandbox_provider: Arc<dyn sandbox::SandboxProvider> =
        if let Ok(ssh_host) = std::env::var("FIRECRACKER_SSH_HOST") {
            let api_url = std::env::var("FIRECRACKER_API_URL")
                .unwrap_or_else(|_| format!("http://{}:8080", ssh_host.split('@').last().unwrap_or(&ssh_host)));
            let ssh_port: u16 = std::env::var("FIRECRACKER_SSH_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(22);
            let ssh_key = std::env::var("FIRECRACKER_SSH_KEY").ok();

            tracing::info!(
                ssh_target = %ssh_host,
                ssh_port = ssh_port,
                api_url = %api_url,
                "initializing Firecracker sandbox provider (RemoteSsh)"
            );

            let remote_state_dir = std::env::var("FC_REMOTE_STATE_DIR")
                .unwrap_or_else(|_| "/var/lib/firecracker".into());
            let remote_fc_bin = std::env::var("FC_REMOTE_BIN")
                .unwrap_or_else(|_| "/usr/local/bin/firecracker".into());

            let fc_config = sandbox::FirecrackerConfig {
                host: sandbox::FirecrackerHostTransportConfig::RemoteSsh {
                    ssh_target: ssh_host,
                    ssh_port,
                    ssh_key_path: ssh_key,
                    api_base_url: api_url,
                    remote_firecracker_bin: remote_fc_bin,
                    remote_state_dir: remote_state_dir.clone(),
                },
                state_dir: base_dir.join("firecracker"),
                kernel_image: std::env::var("FC_KERNEL_IMAGE")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from(format!("{remote_state_dir}/vmlinux"))),
                rootfs_base_image: std::env::var("FC_ROOTFS_IMAGE")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from(format!("{remote_state_dir}/rootfs.ext4"))),
                default_vcpu: std::env::var("FC_VCPU")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1),
                default_memory_mb: std::env::var("FC_MEMORY_MB")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(256),
                network: sandbox::FirecrackerNetworkConfig {
                    enable_internet: true,
                    allowed_egress: vec![],
                    host_port_range_start: 8100,
                    host_port_range_end: 8200,
                },
                use_jailer: false,
                guest_agent: sandbox::GuestAgentTransport::Ssh,
            };
            Arc::new(
                sandbox::backends::firecracker::FirecrackerProvider::new(fc_config)
                    .context("failed to initialize Firecracker sandbox provider")?,
            )
        } else if let Ok(fc_api_url) = std::env::var("FIRECRACKER_API_URL") {
            tracing::info!(
                api_url = %fc_api_url,
                "initializing Firecracker sandbox provider (LimaTcp)"
            );
            let fc_config = sandbox::FirecrackerConfig {
                host: sandbox::FirecrackerHostTransportConfig::LimaTcp {
                    lima_instance: std::env::var("LIMA_INSTANCE").unwrap_or_else(|_| "default".into()),
                    api_base_url: fc_api_url,
                    guest_ssh_via_lima: true,
                },
                state_dir: base_dir.join("firecracker"),
                kernel_image: std::env::var("FC_KERNEL_IMAGE")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| base_dir.join("firecracker/vmlinux")),
                rootfs_base_image: std::env::var("FC_ROOTFS_IMAGE")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| base_dir.join("firecracker/rootfs.ext4")),
                default_vcpu: std::env::var("FC_VCPU")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1),
                default_memory_mb: std::env::var("FC_MEMORY_MB")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(256),
                network: sandbox::FirecrackerNetworkConfig {
                    enable_internet: true,
                    allowed_egress: vec![],
                    host_port_range_start: 8100,
                    host_port_range_end: 8200,
                },
                use_jailer: false,
                guest_agent: sandbox::GuestAgentTransport::Ssh,
            };
            Arc::new(
                sandbox::backends::firecracker::FirecrackerProvider::new(fc_config)
                    .context("failed to initialize Firecracker sandbox provider")?,
            )
        } else {
            tracing::info!("initializing DangerousHost sandbox provider (default)");
            let sandbox_config = sandbox::DangerousConfig {
                root_dir: base_dir.join("sandboxes"),
                ..sandbox::DangerousConfig::default()
            };
            Arc::new(
                sandbox::backends::dangerous::DangerousHostProvider::new(sandbox_config)
                    .context("failed to initialize sandbox provider")?,
            )
        };

    // Create and start the flow scheduler
    let scheduler = Arc::new(FlowScheduler::new(
        store.clone(),
        http_client.clone(),
        github_client.clone(),
        events_tx.clone(),
        sandbox_provider.clone(),
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
        sandbox_provider,
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
