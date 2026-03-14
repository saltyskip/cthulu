#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod hook_script;
mod hook_server;

use tauri::Manager;
use tokio::sync::watch;

/// A signal that becomes true when the backend AppState is fully initialized.
/// Commands should await this before accessing AppState.
pub struct ReadySignal(pub watch::Receiver<bool>);

/// Wait up to 30 seconds for the backend to finish initializing.
pub async fn wait_ready(signal: &tauri::State<'_, ReadySignal>) -> Result<(), String> {
    let mut rx = signal.0.clone();
    if *rx.borrow() {
        return Ok(());
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        rx.wait_for(|ready| *ready),
    )
    .await
    .map_err(|_| "Backend initialization timed out".to_string())?
    .map_err(|_| "Backend initialization failed".to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Resolve static directory from Tauri resource dir
            let resource_dir: Option<std::path::PathBuf> = app.path().resource_dir().ok();
            let static_dir = resource_dir
                .as_ref()
                .map(|d| d.join("static"))
                .filter(|p| p.exists());

            let config = cthulu::ServerConfig {
                port: 0, // Not used in desktop mode
                start_disabled: false,
                static_dir,
                data_dir: None, // Uses ~/.cthulu by default
            };

            let (ready_tx, ready_rx) = watch::channel(false);
            app.manage(ReadySignal(ready_rx));

            // Spawn backend initialization on a dedicated thread with its own
            // tokio runtime (avoids nightly Rust Send issues with tracing).
            let handle_clone = app_handle.clone();
            std::thread::Builder::new()
                .name("cthulu-backend".into())
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .expect("failed to create backend tokio runtime");
                    rt.block_on(async move {
                        if let Err(e) = init_desktop(config, handle_clone, ready_tx).await {
                            eprintln!("[cthulu-studio] backend init error: {e}");
                        }
                    });
                })
                .expect("failed to spawn backend thread");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Flows
            commands::flows::list_flows,
            commands::flows::get_flow,
            commands::flows::create_flow,
            commands::flows::update_flow,
            commands::flows::delete_flow,
            commands::flows::trigger_flow,
            commands::flows::get_flow_runs,
            commands::flows::get_node_types,
            commands::flows::list_prompt_files,
            commands::flows::get_flow_schedule,
            commands::flows::get_scheduler_status,
            commands::flows::validate_cron,
            // Agents
            commands::agents::list_agents,
            commands::agents::get_agent,
            commands::agents::create_agent,
            commands::agents::update_agent,
            commands::agents::delete_agent,
            commands::agents::list_agent_sessions,
            commands::agents::new_agent_session,
            commands::agents::delete_agent_session,
            commands::agents::get_session_status,
            commands::agents::kill_session,
            commands::agents::get_session_log,
            commands::agents::list_session_files,
            commands::agents::read_session_file,
            commands::agents::get_git_snapshot,
            commands::agents::get_git_diff,
            // Chat
            commands::chat::agent_chat,
            commands::chat::stop_agent_chat,
            commands::chat::reconnect_agent_chat,
            // Prompts
            commands::prompts::list_prompts,
            commands::prompts::get_prompt,
            commands::prompts::create_prompt,
            commands::prompts::update_prompt,
            commands::prompts::delete_prompt,
            commands::prompts::summarize_session,
            // Templates
            commands::templates::list_templates,
            commands::templates::get_template_yaml,
            commands::templates::import_template,
            commands::templates::import_yaml,
            commands::templates::import_github,
            // Workflows
            commands::workflows::setup_workflows_repo,
            commands::workflows::list_workspaces,
            commands::workflows::create_workspace,
            commands::workflows::list_workspace_workflows,
            commands::workflows::get_workflow,
            commands::workflows::save_workflow,
            commands::workflows::publish_workflow,
            commands::workflows::delete_workflow,
            commands::workflows::sync_workflows,
            commands::workflows::run_workflow,
            // Agent Repo
            commands::agent_repo::setup_agent_repo,
            commands::agent_repo::list_orgs,
            commands::agent_repo::create_org,
            commands::agent_repo::delete_org,
            commands::agent_repo::list_agent_projects,
            commands::agent_repo::create_agent_project,
            commands::agent_repo::publish_agent,
            commands::agent_repo::unpublish_agent,
            commands::agent_repo::sync_agent_repo,
            // Tasks
            commands::tasks::list_tasks,
            commands::tasks::create_task,
            commands::tasks::update_task,
            commands::tasks::delete_task,
            // Auth
            commands::auth::token_status,
            commands::auth::refresh_token,
            // Secrets
            commands::secrets::get_github_pat_status,
            commands::secrets::save_github_pat,
            commands::secrets::check_setup_status,
            commands::secrets::save_anthropic_key,
            commands::secrets::save_openai_key,
            commands::secrets::save_slack_webhook,
            commands::secrets::save_notion_credentials,
            commands::secrets::save_telegram_credentials,
            // Heartbeat
            commands::heartbeat::wakeup_agent,
            commands::heartbeat::list_heartbeat_runs,
            commands::heartbeat::get_heartbeat_run,
            commands::heartbeat::get_heartbeat_run_log,
            commands::heartbeat::claude_status,
            // Hooks
            commands::hooks::permission_response,
            commands::hooks::list_pending_permissions,
            // Cloud
            commands::cloud::cloud_pool_status,
            commands::cloud::cloud_pool_health,
            commands::cloud::cloud_test_agent,
            // PTY
            commands::pty::spawn_pty,
            commands::pty::write_pty,
            commands::pty::resize_pty,
            commands::pty::kill_pty,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cthulu Studio");
}

/// Initialize the Cthulu backend in pure desktop mode — no HTTP server.
/// AppState is managed by Tauri, commands use IPC, hooks use Unix socket.
async fn init_desktop(
    server_config: cthulu::ServerConfig,
    app_handle: tauri::AppHandle,
    ready_tx: watch::Sender<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cthulu=info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_tree::HierarchicalLayer::new(2)
                .with_targets(true)
                .with_bracketed_fields(false),
        )
        .try_init();

    // Initialize AppState (repos, scheduler, heartbeat, watcher)
    let cthulu::InitResult {
        mut app_state,
        _watcher,
        config: _,
    } = cthulu::init_app_state(server_config).await?;

    // Start Unix domain socket hook server (replaces HTTP hooks)
    match hook_server::start_hook_socket(app_state.clone(), app_handle.clone()).await {
        Ok(socket_path) => {
            match hook_script::generate_hook_script(&socket_path) {
                Ok(script_path) => {
                    println!(
                        "[cthulu-studio] hook socket: {}, script: {}",
                        socket_path.display(),
                        script_path.display()
                    );
                }
                Err(e) => {
                    eprintln!("[cthulu-studio] WARNING: failed to generate hook script: {e}");
                }
            }
            app_state.hook_socket_path = Some(socket_path);
        }
        Err(e) => {
            eprintln!("[cthulu-studio] WARNING: failed to start hook socket: {e}");
        }
    }

    // Start Tauri event bridges (replaces SSE streams)
    events::start_event_bridges(app_handle.clone(), &app_state);

    // Register AppState as Tauri managed state so commands can access it
    app_handle.manage(app_state.clone());

    // Register PTY state (separate from AppState, desktop-only)
    app_handle.manage(commands::pty::PtyState::new());

    let _ = ready_tx.send(true);

    println!("[cthulu-studio] backend initialized (pure Tauri IPC mode, no HTTP server)");

    // Keep this task alive — dropping _watcher stops the filesystem watcher.
    // We park here until the app exits.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    // The sender is dropped when the app exits (thread terminates)
    std::mem::forget(tx);
    let _ = rx.await;

    // Cleanup on exit
    if let Some(ref socket_path) = app_state.hook_socket_path {
        hook_server::cleanup_hook_socket(socket_path);
    }
    cthulu::shutdown_app_state(&app_state).await;

    Ok(())
}
