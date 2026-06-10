use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use its_hub_rs::server;
use its_hub_rs::server::state::AppState;

#[derive(Parser)]
#[command(name = "its-hub-rs", about = "Inference-Time Scaling in Rust")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8108)]
    port: u16,
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => { tracing::info!("received SIGINT, shutting down"); }
        _ = sigterm.recv() => { tracing::info!("received SIGTERM, shutting down"); }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
        tracing::info!("received Ctrl+C, shutting down");
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let cli = Cli::parse();
    let state = Arc::new(AppState::new());
    let app = server::app(state);
    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("its-hub-rs listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}
