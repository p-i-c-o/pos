//! Server-side PostgreSQL connection, migrations and instance metadata.
//!
//! This crate deliberately does NOT implement task/project repositories or
//! shared data access before authentication and tenant policy are defined.

mod auth;
pub use auth::{Identity, IdentityError, Login};

use std::{error::Error, fmt, str::FromStr, time::Duration};

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// A safe-to-display error category. Never formats credentials or a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostgresError {
    InvalidConfiguration,
    ConnectionFailed,
    MigrationFailed,
    QueryFailed,
}

impl fmt::Display for PostgresError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration => f.write_str("invalid PostgreSQL connection URL"),
            Self::ConnectionFailed => f.write_str("PostgreSQL connection failed"),
            Self::MigrationFailed => f.write_str("PostgreSQL schema migration failed"),
            Self::QueryFailed => f.write_str("PostgreSQL instance query failed"),
        }
    }
}
impl Error for PostgresError {}

/// Non-sensitive instance metadata. Never contains a user or device identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInstance {
    pub id: String,
    pub created_at: String,
}

/// Small, bounded connection pool for future authenticated server services.
#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Connect to an explicitly supplied database URL, apply the versioned
    /// migration and verify the single instance row. Does not create a
    /// database, users, tasks, projects or any business-data API.
    ///
    /// This changes the schema of the database specified by the caller.
    /// Only pass a dedicated POS server database with a restricted role.
    pub async fn open(database_url: &str) -> Result<Self, PostgresError> {
        // Do not let arbitrary strings be silently interpreted as sockets,
        // env overrides or unanticipated connection settings.
        if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            return Err(PostgresError::InvalidConfiguration);
        }
        let options = PgConnectOptions::from_str(database_url)
            .map_err(|_| PostgresError::InvalidConfiguration)?
            .application_name("pos-server");

        if options.get_database().is_none_or(str::is_empty) {
            return Err(PostgresError::InvalidConfiguration);
        }

        let pool = PgPoolOptions::new()
            .min_connections(0)
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(|_| PostgresError::ConnectionFailed)?;

        if sqlx::migrate!("./migrations").run(&pool).await.is_err() {
            pool.close().await;
            return Err(PostgresError::MigrationFailed);
        }

        let store = Self { pool };
        if let Err(error) = store.instance().await {
            store.pool.close().await;
            return Err(error);
        }
        Ok(store)
    }

    /// Checks DB reachability and reads the persistent server instance marker.
    /// This is a DB health probe, not an authentication or sync readiness check.
    pub async fn instance(&self) -> Result<ServerInstance, PostgresError> {
        let row = sqlx::query(
            "SELECT instance_id::text AS id, created_at::text AS created_at \
             FROM pos_server_instance WHERE singleton = TRUE",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| PostgresError::QueryFailed)?;

        Ok(ServerInstance {
            id: row.try_get("id").map_err(|_| PostgresError::QueryFailed)?,
            created_at: row
                .try_get("created_at")
                .map_err(|_| PostgresError::QueryFailed)?,
        })
    }

    /// Drain and close pooled connections for controlled server shutdown.
    pub async fn close(self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_urls_fail_without_network_access_or_secret_echo() {
        for url in [
            "",
            "not-a-url",
            "sqlite://ignored",
            "postgres://",
            "postgres://invalid:secret@127.0.0.1/",
        ] {
            let result = PostgresStore::open(url).await;
            assert!(matches!(result, Err(PostgresError::InvalidConfiguration)));
        }

        let error = PostgresError::ConnectionFailed;
        assert!(!error.to_string().contains("password"));
        assert_eq!(error.to_string(), "PostgreSQL connection failed");
    }
}
