//! Integration tests require an explicitly named, disposable local CI database.
//! Never point POS_TEST_DATABASE_URL at a production or personal database.

use std::str::FromStr;

use pos_postgres::PostgresStore;
use sqlx::{PgPool, postgres::PgConnectOptions};

#[tokio::test]
async fn creates_and_reuses_instance_identity() {
    let Ok(url) = std::env::var("POS_TEST_DATABASE_URL") else {
        eprintln!("PostgreSQL integration skipped: POS_TEST_DATABASE_URL is unset");
        return;
    };

    // Defense against accidentally migrating a live database from tests.
    let options = PgConnectOptions::from_str(&url).expect("invalid test database URL");
    assert_eq!(
        options.get_database(),
        Some("pos_ci"),
        "integration tests require the disposable pos_ci database"
    );
    assert!(
        matches!(options.get_host(), "localhost" | "127.0.0.1" | "::1"),
        "integration tests require a local PostgreSQL listener"
    );

    let (first, second) = tokio::join!(PostgresStore::open(&url), PostgresStore::open(&url));
    let first = first.expect("initial concurrent migration failed");
    let second = second.expect("concurrent migration failed");

    let instance = first.instance().await.expect("instance metadata missing");
    let instance_again = second.instance().await.unwrap();
    assert_eq!(instance, instance_again, "instance identity must be stable");
    assert_eq!(instance.id.len(), 36, "instance ID should be a UUID");
    assert!(!instance.created_at.is_empty());

    let raw = PgPool::connect(&url).await.unwrap();
    let instances: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_server_instance")
        .fetch_one(&raw)
        .await
        .unwrap();
    assert_eq!(instances, 1, "must create exactly one instance");

    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success = true")
            .fetch_one(&raw)
            .await
            .unwrap();
    assert_eq!(
        applied, 1,
        "the initial migration must be applied exactly once"
    );

    for name in ["pos_users", "pos_workspaces", "pos_tasks", "pos_projects"] {
        let exists: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(name)
            .fetch_one(&raw)
            .await
            .unwrap();
        assert!(exists.is_none(), "unexpected business-data table: {name}");
    }

    raw.close().await;
    first.close().await;
    second.close().await;
}
