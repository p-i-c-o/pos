//! Local POS server and trusted operator-only account administration.

use std::error::Error;

mod admin;

use admin::{Cli, Command};
use clap::Parser;
use pos_postgres::PostgresStore;
use pos_server::{ServerConfig, bind, serve_with_store};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    let config = ServerConfig {
        listen: args.listen,
    };
    if let Some(Command::Admin { action }) = args.action {
        return admin::run_admin(action).await;
    }
    // Reject unsafe addresses BEFORE opening/migrating a configured database.
    config.validate()?;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("pos_server=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    // Bind locally before migrating, so a port collision cannot trigger
    // unnecessary schema changes. No connections are accepted until ready.
    let listener = bind(&config).await.map_err(|error| {
        tracing::error!(%error, listen = %config.listen, "server startup failed");
        error
    })?;

    // An explicitly configured database is required for authenticated routes.
    // Open applies versioned migrations: always use a dedicated, backed-up DB.
    // Never log connection strings or database credentials.
    let store = match std::env::var("POS_SERVER_DATABASE_URL") {
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return Err("invalid POS_SERVER_DATABASE_URL encoding".into()),
        Ok(url) => {
            if url.is_empty() {
                return Err("POS_SERVER_DATABASE_URL is empty".into());
            }
            Some(PostgresStore::open(&url).await.inspect_err(|_error| {
                tracing::error!("database initialization failed");
            })?)
        }
    };

    tracing::info!(
        address = %listener.local_addr()?,
        version = env!("CARGO_PKG_VERSION"),
        "server started; no authenticated business API is available"
    );

    serve_with_store(listener, shutdown_signal(), store).await?;
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
