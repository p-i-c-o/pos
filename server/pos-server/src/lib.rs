//! Local-only POS HTTP foundation. Read-only workspace APIs require a
//! configured PostgreSQL connection and a valid, active device-bound session.

mod api;

use pos_postgres::PostgresStore;
use std::{fmt, future::Future, io, net::SocketAddr};

use axum::{
    Json, Router,
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::get,
};
use clap::Parser;
use serde::Serialize;
use tokio::net::TcpListener;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:8787";
pub const API_VERSION: &str = "v1";

/// Runtime options. Explicit non-loopback addresses are rejected until
/// authentication, authorization and TLS are implemented.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "pos-server",
    version,
    about = "POS local HTTP server foundation"
)]
pub struct ServerConfig {
    /// Local socket address, or POS_SERVER_LISTEN (defaults to 127.0.0.1:8787).
    #[arg(long, env = "POS_SERVER_LISTEN", default_value = DEFAULT_LISTEN)]
    pub listen: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationError(&'static str);

impl fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ConfigurationError {}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !self.listen.ip().is_loopback() {
            return Err(ConfigurationError(
                "refusing non-loopback listen address: authentication and TLS are not implemented",
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    service: &'static str,
    version: &'static str,
    api_version: &'static str,
}
#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: &'static str,
}
#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

/// A health result proves HTTP reachability, NOT data readiness.
async fn health() -> (HeaderMap, Json<Health>) {
    let mut headers = HeaderMap::new();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    (
        headers,
        Json(Health {
            status: "ok",
            service: pos_core::PRODUCT_NAME,
            version: env!("CARGO_PKG_VERSION"),
            api_version: API_VERSION,
        }),
    )
}

async fn not_found() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: ErrorDetail {
                code: "not_found",
                message: "route not found",
            },
        }),
    )
}

/// Health-only router used when no PostgreSQL database is configured.
pub fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/health", get(health))
        .fallback(not_found)
}

/// Adds authenticated read-only routes, while preserving health behavior.
/// The store must have completed migrations before this router is created.
pub fn router_with_store(store: PostgresStore) -> Router {
    router().merge(api::routes(store))
}

/// Validate BEFORE opening a network listener.
pub async fn bind(config: &ServerConfig) -> io::Result<TcpListener> {
    config
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    TcpListener::bind(config.listen).await
}

/// Drain in-flight HTTP requests before shutdown. The caller supplies the
/// shutdown future, allowing deterministic lifecycle tests.
pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to serve on a non-loopback listener",
        ));
    }
    serve_with_store(listener, shutdown, None).await
}

/// Run the optional authenticated HTTP router on an already-bound loopback
/// socket. No business data is exposed unless the database was configured.
pub async fn serve_with_store(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
    store: Option<PostgresStore>,
) -> io::Result<()> {
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to serve on a non-loopback listener",
        ));
    }
    let auth_enabled = store.is_some();
    let closing_store = store.clone();
    let app = store.map_or_else(router, router_with_store);
    tracing::info!(%address, api_version = API_VERSION, auth_enabled, "POS HTTP listening");
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await;
    if let Some(store) = closing_store {
        store.close().await;
    }
    result
}
