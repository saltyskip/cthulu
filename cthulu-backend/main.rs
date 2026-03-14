use clap::Parser;
use dotenvy::dotenv;
use std::error::Error;

use cthulu::ServerConfig;

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv().ok();

    // Parse CLI args — default to Serve when no subcommand is given,
    // but still allow --help and --version to work.
    let args: Vec<String> = std::env::args().collect();
    let cli = if args.len() <= 1 {
        // No subcommand given, default to serve
        Cli::Serve {
            start_disabled: false,
        }
    } else {
        Cli::parse()
    };

    match cli {
        Cli::Serve { start_disabled } => run_server(start_disabled).await,
    }
}

async fn run_server(start_disabled: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8081);

    let config = ServerConfig {
        port,
        start_disabled,
        static_dir: None,
        data_dir: None,
    };

    // Create a watch channel for shutdown signaling
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn a task that listens for ctrl-c / SIGTERM and sends shutdown
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    cthulu::start_server(config, shutdown_rx).await?;

    // Force exit — spawn_blocking reader threads can't be stopped gracefully
    std::process::exit(0);

    #[allow(unreachable_code)]
    Ok(())
}

/// Wait for Ctrl+C or SIGTERM to initiate graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
    tracing::info!("shutdown signal received");
}
