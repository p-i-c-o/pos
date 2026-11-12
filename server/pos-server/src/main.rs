//! Local-only POS server entry point. Data APIs require future auth.

use std::error::Error;

use clap::Parser;
use pos_server::{ServerConfig, bind, serve};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = ServerConfig::parse();
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("pos_server=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let listener = bind(&config).await.map_err(|error| {
        tracing::error!(%error, listen = %config.listen, "server startup failed");
        error
    })?;

    tracing::info!(
        address = %listener.local_addr()?,
        version = env!("CARGO_PKG_VERSION"),
        "server started; no authenticated business API is available"
    );

    serve(listener, shutdown_signal()).await?;
    tracing::info!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler unavailable");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("Ctrl-C handler unavailable");
    }
    tracing::info!("shutdown signal received");
}
