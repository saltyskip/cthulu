pub mod agent_sdk;
pub mod agents;
pub mod claude_adapter;
pub mod config;
pub mod flows;
pub mod git;
pub mod github;
pub mod prompts;
pub mod sandbox;
pub mod api;
pub mod tasks;
pub mod templates;
pub mod watcher;
pub mod cloud;

use anyhow::Context;
use axum::body::Body;
use axum::extract::Request;
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::agents::file_repository::FileAgentRepository;
use crate::agents::repository::AgentRepository;
use crate::agents::{STUDIO_ASSISTANT_ID, default_studio_assistant};
use crate::api::changes::ResourceChangeEvent;
use crate::flows::events::RunEvent;
use crate::flows::file_repository::FileFlowRepository;
use crate::flows::repository::FlowRepository;
use crate::flows::scheduler::FlowScheduler;
use crate::github::client::{GithubClient, HttpGithubClient};
use crate::prompts::file_repository::FilePromptRepository;
use crate::prompts::repository::PromptRepository;

/// Configuration for starting the Cthulu server from an external host (e.g. Tauri).
pub struct ServerConfig {
    pub port: u16,
    pub start_disabled: bool,
    /// Override for static directory path. If None, uses auto-detection.
    pub static_dir: Option<std::path::PathBuf>,
    /// Override for data directory path. If None, uses ~/.cthulu.
    pub data_dir: Option<std::path::PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8081,
            start_disabled: false,
            static_dir: None,
            data_dir: None,
        }
    }
}

/// Result of `init_app_state()` — everything needed to run the backend
/// in either HTTP-server mode or embedded Tauri desktop mode.
pub struct InitResult {
    pub app_state: api::AppState,
    /// Keep this alive — dropping it stops the filesystem watcher.
    pub _watcher: watcher::FileChangeWatcher,
    /// Concrete repos (needed if you want to pass them around separately).
    pub config: config::Config,
}

/// Initialize the full AppState and background services (scheduler, heartbeat,
/// file watcher) **without** starting an HTTP server.
///
/// Both `start_server()` and Tauri's `main.rs` call this to share the same
/// initialization path.
pub async fn init_app_state(
    server_config: ServerConfig,
) -> std::result::Result<InitResult, Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();

    let config = config::Config::from_env();
    let port = server_config.port;

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

    // Initialize data directory — use override or default to ~/.cthulu
    let base_dir = server_config.data_dir.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".cthulu")
    });

    // Initialize repositories (file-backed)
    let file_flow_repo = Arc::new(FileFlowRepository::new(base_dir.clone()));
    file_flow_repo
        .load_all()
        .await
        .context("failed to load flow repository")?;
    let flow_repo: Arc<dyn FlowRepository> = file_flow_repo.clone();

    let file_prompt_repo = Arc::new(FilePromptRepository::new(base_dir.clone()));
    file_prompt_repo
        .load_all()
        .await
        .context("failed to load prompt repository")?;
    let prompt_repo: Arc<dyn PromptRepository> = file_prompt_repo.clone();

    let file_agent_repo = Arc::new(FileAgentRepository::new(base_dir.clone()));
    file_agent_repo
        .load_all()
        .await
        .context("failed to load agent repository")?;
    let agent_repo: Arc<dyn AgentRepository> = file_agent_repo.clone();

    // Seed or migrate the built-in Studio Assistant with sub-agents.
    match agent_repo.get(STUDIO_ASSISTANT_ID).await {
        None => {
            tracing::info!("seeding built-in Studio Assistant agent with sub-agents");
            agent_repo
                .save(default_studio_assistant())
                .await
                .with_context(|| "failed to seed Studio Assistant agent")?;
        }
        Some(existing) if existing.subagents.is_empty() => {
            tracing::info!("migrating Studio Assistant: adding default sub-agents");
            let mut updated = existing;
            updated.subagents = crate::agents::default_subagents();
            updated.updated_at = chrono::Utc::now();
            agent_repo
                .save(updated)
                .await
                .with_context(|| "failed to migrate Studio Assistant with sub-agents")?;
        }
        Some(_) => {} // Already has sub-agents, nothing to do
    }

    let (events_tx, _) = tokio::sync::broadcast::channel::<RunEvent>(256);
    let (changes_tx, _) = tokio::sync::broadcast::channel::<ResourceChangeEvent>(256);

    // Load persisted interact sessions from ~/.cthulu/sessions.yaml
    let sessions_path = base_dir.join("sessions.yaml");
    let persisted_sessions = api::load_sessions(&sessions_path);

    // Load GitHub PAT from secrets.json (if configured)
    let secrets_path = base_dir.join("secrets.json");
    let github_pat: Option<String> = if secrets_path.exists() {
        std::fs::read_to_string(&secrets_path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|v| v["github_pat"].as_str().map(String::from))
    } else {
        None
    };
    if github_pat.is_some() {
        tracing::info!("GitHub PAT loaded from secrets.json");
    }

    // Read OAuth token: macOS Keychain first, then CLAUDE_CODE_OAUTH_TOKEN env
    let oauth_token: Option<String> = {
        let keychain_result = std::process::Command::new("security")
            .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .output();
        match keychain_result {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let token = v["claudeAiOauth"]["accessToken"]
                        .as_str()
                        .map(String::from);
                    if token.is_some() {
                        tracing::info!("OAuth token loaded from macOS Keychain");
                    }
                    token
                } else {
                    None
                }
            }
            _ => None,
        }
        .or_else(|| {
            std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok().map(|t| {
                tracing::info!("OAuth token loaded from CLAUDE_CODE_OAUTH_TOKEN env");
                t
            })
        })
    };

    // Initialize sandbox provider
    let sandbox_provider: Arc<dyn sandbox::SandboxProvider> =
        if let Ok(ssh_host) = std::env::var("FIRECRACKER_SSH_HOST") {
            let api_url = std::env::var("FIRECRACKER_API_URL").unwrap_or_else(|_| {
                format!(
                    "http://{}:8080",
                    ssh_host.split('@').last().unwrap_or(&ssh_host)
                )
            });
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

            let kernel_default =
                std::path::PathBuf::from(format!("{remote_state_dir}/vmlinux"));
            let rootfs_default =
                std::path::PathBuf::from(format!("{remote_state_dir}/rootfs.ext4"));

            let fc_config = build_fc_config(
                sandbox::FirecrackerHostTransportConfig::RemoteSsh {
                    ssh_target: ssh_host,
                    ssh_port,
                    ssh_key_path: ssh_key,
                    api_base_url: api_url,
                    remote_firecracker_bin: remote_fc_bin,
                    remote_state_dir: remote_state_dir.clone(),
                },
                &base_dir,
                kernel_default,
                rootfs_default,
            );
            Arc::new(
                sandbox::backends::firecracker::FirecrackerProvider::new(fc_config)
                    .context("failed to initialize Firecracker sandbox provider")?,
            )
        } else if let Ok(fc_api_url) = std::env::var("FIRECRACKER_API_URL") {
            tracing::info!(
                api_url = %fc_api_url,
                "initializing Firecracker sandbox provider (LimaTcp)"
            );

            let kernel_default = base_dir.join("firecracker/vmlinux");
            let rootfs_default = base_dir.join("firecracker/rootfs.ext4");

            let fc_config = build_fc_config(
                sandbox::FirecrackerHostTransportConfig::LimaTcp {
                    lima_instance: std::env::var("LIMA_INSTANCE")
                        .unwrap_or_else(|_| "default".into()),
                    api_base_url: fc_api_url,
                    guest_ssh_via_lima: true,
                },
                &base_dir,
                kernel_default,
                rootfs_default,
            );
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

    // Session streams for flow-run session broadcasting
    let session_streams = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

    // Interact sessions (shared between scheduler and AppState)
    let interact_sessions = Arc::new(tokio::sync::RwLock::new(persisted_sessions));

    // Create and start the flow scheduler
    let scheduler = Arc::new(FlowScheduler::new(
        flow_repo.clone(),
        http_client.clone(),
        github_client.clone(),
        events_tx.clone(),
        sandbox_provider.clone(),
        agent_repo.clone(),
        interact_sessions.clone(),
        sessions_path.clone(),
        base_dir.clone(),
        session_streams.clone(),
    ));
    if server_config.start_disabled {
        tracing::info!("Starting with all flow triggers disabled (--start-disabled)");
        let flows = flow_repo.list_flows().await;
        for mut flow in flows {
            if flow.enabled {
                flow.enabled = false;
                if let Err(e) = flow_repo.save_flow(flow).await {
                    tracing::warn!(error = %e, "Failed to disable flow");
                }
            }
        }
    } else {
        scheduler.start_all().await;
    }

    // Resolve static/ directory
    let static_dir = if let Some(dir) = server_config.static_dir {
        dir
    } else {
        std::env::var("CTHULU_STATIC_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let cwd_static = std::env::current_dir()
                    .unwrap_or_else(|_| ".".into())
                    .join("static");
                if cwd_static.exists() {
                    cwd_static
                } else {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.join("static")))
                        .unwrap_or_else(|| std::path::PathBuf::from("static"))
                }
            })
    };

    tracing::info!(path = %static_dir.display(), "static directory");

    // Pre-load template metadata into memory
    let template_cache = crate::templates::load_templates(&static_dir);
    tracing::info!(count = template_cache.len(), "pre-loaded template cache");

    // Initialize cloud VM pool (optional)
    let vm_pool: Option<std::sync::Arc<cloud::VmPool>> =
        match std::env::var("VM_MANAGER_URL").ok().filter(|s| !s.is_empty()) {
            Some(url) => {
                let pool_size: usize = std::env::var("VM_POOL_SIZE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5);
                let client = cloud::VmManagerClient::new(&url);
                match cloud::VmPool::init(client, pool_size).await {
                    Ok(pool) => {
                        tracing::info!(pool_size = pool.total_count().await, "cloud VM pool initialized");
                        Some(pool)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to initialize cloud VM pool — cloud execution disabled");
                        None
                    }
                }
            }
            None => {
                tracing::info!("VM_MANAGER_URL not set — cloud execution disabled");
                None
            }
        };

    let a2a_client = std::sync::Arc::new(cloud::A2aClient::new());

    // Initialize file-based task store for agent assignments.
    let task_store = Arc::new(crate::agents::tasks::TaskFileStore::new(&base_dir).await);

    // Initialize heartbeat scheduler for autonomous agent runs.
    let heartbeat_scheduler = crate::agents::heartbeat::HeartbeatScheduler::new(
        agent_repo.clone(),
        base_dir.clone(),
    );
    heartbeat_scheduler.start_all().await;
    let heartbeat_scheduler = Arc::new(tokio::sync::RwLock::new(heartbeat_scheduler));

    let app_state = api::AppState {
        github_client,
        http_client,
        flow_repo,
        prompt_repo,
        agent_repo,
        scheduler,
        events_tx,
        changes_tx: changes_tx.clone(),
        interact_sessions,
        sessions_path,
        data_dir: base_dir.clone(),
        static_dir,
        template_cache: Arc::new(tokio::sync::RwLock::new(template_cache)),
        live_processes: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        sandbox_provider,
        oauth_token: Arc::new(tokio::sync::RwLock::new(oauth_token)),
        session_streams,
        chat_event_buffers: Arc::new(tokio::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        sdk_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        pending_permissions: Arc::new(tokio::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        global_hook_tx: Arc::new(tokio::sync::broadcast::channel::<String>(256).0),
        server_port: port,
        hook_socket_path: None,
        github_pat: Arc::new(tokio::sync::RwLock::new(github_pat)),
        secrets_path,
        cors_origins: config.cors_origins.clone(),
        environment: config.environment.clone(),
        vm_pool,
        a2a_client,
        heartbeat_scheduler,
        task_store,
        workflow_self_writes: Arc::new(watcher::WorkflowSelfWrites::new()),
    };

    // Start file change watcher (keeps caches in sync with external edits)
    let _watcher = watcher::FileChangeWatcher::start(
        base_dir,
        file_flow_repo,
        file_agent_repo,
        file_prompt_repo,
        changes_tx,
        app_state.workflow_self_writes.clone(),
    )
    .context("failed to start file change watcher")?;

    Ok(InitResult {
        app_state,
        _watcher,
        config,
    })
}

/// Gracefully shut down the backend: stop heartbeat scheduler, kill child processes,
/// disconnect SDK sessions.
pub async fn shutdown_app_state(app_state: &api::AppState) {
    tracing::info!("shutting down: stopping heartbeat scheduler and killing child processes");
    {
        let scheduler = app_state.heartbeat_scheduler.read().await;
        scheduler.stop_all().await;
    }
    {
        let mut pool = app_state.live_processes.lock().await;
        for (key, mut proc) in pool.drain() {
            if let Err(e) = proc.child.kill().await {
                tracing::trace!(key = %key, error = %e, "live process kill on shutdown");
            }
        }
    }
    {
        let mut pool = app_state.sdk_sessions.lock().await;
        for (key, mut session) in pool.drain() {
            if let Err(e) = session.disconnect().await {
                tracing::trace!(key = %key, error = %e, "SDK session disconnect on shutdown");
            }
        }
    }
}

/// Start the Cthulu HTTP server with the given configuration.
///
/// The caller provides a `shutdown` watch receiver — sending `true` on the
/// corresponding sender triggers graceful shutdown.  This allows a Tauri app
/// (or any other host) to control the server lifecycle without relying on
/// Unix signals.
pub async fn start_server(
    server_config: ServerConfig,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cthulu=info,tower_http=warn,hyper=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_tree::HierarchicalLayer::new(2)
                .with_targets(true)
                .with_bracketed_fields(false),
        )
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

    let port = server_config.port;

    let InitResult { app_state, _watcher, config } = init_app_state(server_config).await?;

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

    let app = api::create_app(app_state.clone())
        .layer(SentryHttpLayer::new().enable_transaction())
        .layer(NewSentryLayer::<Request<Body>>::new_from_top());

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on http://{addr}");

    // Graceful shutdown: wait for the watch receiver to see `true`
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            loop {
                if *shutdown.borrow() {
                    break;
                }
                if shutdown.changed().await.is_err() {
                    break;
                }
            }
            tracing::info!("shutdown signal received");
        })
        .await?;

    shutdown_app_state(&app_state).await;

    Ok(())
}

/// Build a `FirecrackerConfig` with the transport-specific `host` variant and
/// shared defaults for vcpu, memory, network, jailer, and guest agent.
///
/// `kernel_default` / `rootfs_default` are the fallback paths when the
/// corresponding env vars (`FC_KERNEL_IMAGE`, `FC_ROOTFS_IMAGE`) are not set.
pub fn build_fc_config(
    host: sandbox::FirecrackerHostTransportConfig,
    base_dir: &std::path::Path,
    kernel_default: std::path::PathBuf,
    rootfs_default: std::path::PathBuf,
) -> sandbox::FirecrackerConfig {
    sandbox::FirecrackerConfig {
        host,
        state_dir: base_dir.join("firecracker"),
        kernel_image: std::env::var("FC_KERNEL_IMAGE")
            .map(std::path::PathBuf::from)
            .unwrap_or(kernel_default),
        rootfs_base_image: std::env::var("FC_ROOTFS_IMAGE")
            .map(std::path::PathBuf::from)
            .unwrap_or(rootfs_default),
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
    }
}
