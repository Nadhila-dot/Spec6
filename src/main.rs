mod agent;
mod auth;
mod chat;
mod cognee;
mod config;
mod db;
mod inference;
mod overview;
mod prompt;
mod render;
mod routes;
mod triggerware;
mod speechmatics;
mod watchtower;

use anyhow::{Context, Result};
use tokio::{net::TcpListener, signal};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = config::AppConfig::from_env()?;
    let bind_address = config.bind_address();
    let state = render::AppState::boot(config.clone()).await?;
    watchtower::spawn(state.clone());
    triggerware::spawn(state.clone());
    let app = routes::router(state);
    let listener = TcpListener::bind(&bind_address)
        .await
        .with_context(|| format!("failed to bind {bind_address}"))?;

    tracing::info!(
        environment = %config.env.as_str(),
        bind_address = %bind_address,
        app_url = %config.app_url,
        "server ready"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server stopped unexpectedly")
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("win_win=info,tower_http=info"));

    fmt().with_env_filter(filter).compact().init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut stream = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        stream.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
