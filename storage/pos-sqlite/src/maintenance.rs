//! Local database integrity checks and safely published SQLite backup/restore.
//!
//! Use VACUUM INTO for a consistent snapshot that includes committed WAL data.
//! All destinations are new files: never overwrite, remove or mutate a live DB.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tempfile::{Builder, NamedTempFile};

use pos_storage::StorageError;

use super::storage_err;

pub struct SqliteMaintenance;

impl SqliteMaintenance {
    /// Validate an existing POS database read-only, without applying migrations.
    pub async fn check(source: &Path) -> Result<(), StorageError> {
        let pool = read_only_pool(source).await?;
        check_pool(&pool).await?;
        pool.close().await;
        Ok(())
    }

    /// Snapshot an open/WAL-mode SQLite database using SQLite VACUUM INTO.
    /// The backup becomes visible only after successful validation and sync.
    /// Existing files, even empty ones, are never overwritten.
    pub async fn backup(source: &Path, destination: &Path) -> Result<(), StorageError> {
        ensure_unused_destination(destination)?;
        let pool = read_only_pool(source).await?;
        check_pool(&pool).await?;

        let staged = staging_file(destination)?;
        let stage_path = sqlite_path(staged.path())?;
        sqlx::query("VACUUM INTO ?")
            .bind(stage_path)
            .execute(&pool)
            .await
            .map_err(storage_err)?;
        pool.close().await;

        Self::check(staged.path()).await?;
        publish(staged, destination)?;
        Ok(())
    }

    /// Recover into a *new* SQLite path, never touching the active database.
    /// Validate the copied file before publishing it atomically without overwrite.
    pub async fn restore(source: &Path, destination: &Path) -> Result<(), StorageError> {
        ensure_unused_destination(destination)?;
        Self::check(source).await?;

        let mut staged = staging_file(destination)?;
        let mut input = File::open(source).map_err(storage_err)?;
        io::copy(&mut input, staged.as_file_mut()).map_err(storage_err)?;
        staged.as_file_mut().flush().map_err(storage_err)?;
        Self::check(staged.path()).await?;
        publish(staged, destination)?;
        Ok(())
    }
}

fn sqlite_path(path: &Path) -> Result<&str, StorageError> {
    path.to_str()
        .ok_or_else(|| StorageError("SQLite file paths must be UTF-8".into()))
}

fn ensure_unused_destination(path: &Path) -> Result<(), StorageError> {
    if fs::symlink_metadata(path).is_ok() {
        return Err(StorageError(format!(
            "destination already exists; refusing to overwrite: {}",
            path.display()
        )));
    }
    let parent = parent_dir(path);
    if !parent.is_dir() {
        return Err(StorageError(format!(
            "destination directory does not exist: {}",
            parent.display()
        )));
    }
    Ok(())
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn staging_file(destination: &Path) -> Result<NamedTempFile, StorageError> {
    Builder::new()
        .prefix(".pos-staging-")
        .tempfile_in(parent_dir(destination))
        .map_err(storage_err)
}

fn publish(staged: NamedTempFile, destination: &Path) -> Result<(), StorageError> {
    staged.as_file().sync_all().map_err(storage_err)?;
    // persist_noclobber uses a no-replace operation even if another process
    // creates the requested destination after the initial existence check.
    let published = staged.persist_noclobber(destination).map_err(storage_err)?;
    published.sync_all().map_err(storage_err)?;
    // Sync the directory entry so a completed backup survives a machine crash.
    File::open(parent_dir(destination))
        .and_then(|directory| directory.sync_all())
        .map_err(storage_err)?;
    Ok(())
}

pub(crate) async fn read_only_pool(path: &Path) -> Result<SqlitePool, StorageError> {
    let metadata = fs::metadata(path).map_err(storage_err)?;
    if !metadata.is_file() {
        return Err(StorageError(format!(
            "not a regular SQLite database file: {}",
            path.display()
        )));
    }
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .read_only(true)
                .foreign_keys(true),
        )
        .await
        .map_err(storage_err)
}

async fn check_pool(pool: &SqlitePool) -> Result<(), StorageError> {
    let findings: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(pool)
        .await
        .map_err(storage_err)?;
    if findings.len() != 1 || findings[0] != "ok" {
        return Err(StorageError(format!(
            "SQLite integrity check failed: {}",
            findings.join("; ")
        )));
    }

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .map_err(storage_err)?;
    if !violations.is_empty() {
        let first = &violations[0];
        let table: String = first.try_get("table").map_err(storage_err)?;
        return Err(StorageError(format!(
            "{} foreign-key violations (first table: {table})",
            violations.len()
        )));
    }

    // Refuse arbitrary SQLite databases even when their integrity is otherwise OK.
    let required: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master \
         WHERE type = 'table' AND name IN ('tasks', '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await
    .map_err(storage_err)?;
    if required != 2 {
        return Err(StorageError(
            "not a POS SQLite database (missing tasks or migration history)".into(),
        ));
    }

    let latest: Option<i64> =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(pool)
            .await
            .map_err(storage_err)?;
    if latest.is_some_and(|version| version >= 20261112010000) {
        let projects: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'projects'",
        )
        .fetch_one(pool)
        .await
        .map_err(storage_err)?;
        if projects != 1 {
            return Err(StorageError(
                "POS project migration is recorded but the projects table is missing".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteTaskRepository;
    use pos_projects::NewProject;
    use pos_storage::{ProjectRepository, TaskRepository};
    use pos_tasks::NewTask;
    use sqlx::Row;
    use tempfile::tempdir;

    #[tokio::test]
    async fn backup_and_restore_preserve_wal_writes_and_relationships() {
        let directory = tempdir().unwrap();
        let live = directory.path().join("live.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        let restored = directory.path().join("restored.sqlite3");

        let store = SqliteTaskRepository::open(&live).await.unwrap();
        let project = store
            .create_project(NewProject::new("University", None).unwrap())
            .await
            .unwrap();
        let mut new_task = NewTask::new("Write report", Some("2026-12-17")).unwrap();
        new_task.project_id = Some(project.id);
        store.create(new_task).await.unwrap();

        // Keep a pool open and the source in WAL mode while taking the snapshot.
        SqliteMaintenance::backup(&live, &backup).await.unwrap();
        SqliteMaintenance::check(&backup).await.unwrap();
        SqliteMaintenance::restore(&backup, &restored)
            .await
            .unwrap();
        SqliteMaintenance::check(&restored).await.unwrap();

        let recovered = SqliteTaskRepository::open(&restored).await.unwrap();
        let tasks = recovered.list(true).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].project_id, Some(project.id));
        assert_eq!(tasks[0].due_date.as_deref(), Some("2026-12-17"));
        assert_eq!(recovered.list_projects(true).await.unwrap().len(), 1);

        // A restored database is independently writable.
        recovered
            .create(NewTask::new("Post-recovery", None).unwrap())
            .await
            .unwrap();
        assert_eq!(store.list(true).await.unwrap().len(), 1);
        assert_eq!(recovered.list(true).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn existing_output_is_never_overwritten_and_source_survives_failures() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("live.sqlite3");
        let output = dir.path().join("output.sqlite3");
        let store = SqliteTaskRepository::open(&live).await.unwrap();
        store
            .create(NewTask::new("keep", None).unwrap())
            .await
            .unwrap();

        fs::write(&output, b"sentinel").unwrap();
        assert!(SqliteMaintenance::backup(&live, &output).await.is_err());
        assert!(SqliteMaintenance::restore(&live, &output).await.is_err());
        assert_eq!(fs::read(&output).unwrap(), b"sentinel");

        assert!(SqliteMaintenance::backup(&live, &live).await.is_err());
        assert!(SqliteMaintenance::restore(&live, &live).await.is_err());
        assert_eq!(store.list(true).await.unwrap()[0].title, "keep");

        let destination = dir.path().join("no-parent").join("backup.sqlite3");
        assert!(
            SqliteMaintenance::backup(&live, &destination)
                .await
                .is_err()
        );
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dangling_symlinks_are_treated_as_existing_destinations() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let live = dir.path().join("live.sqlite3");
        SqliteTaskRepository::open(&live).await.unwrap();
        let link = dir.path().join("backup.sqlite3");
        symlink("not-a-real-file", &link).unwrap();
        assert!(SqliteMaintenance::backup(&live, &link).await.is_err());
        assert!(SqliteMaintenance::restore(&live, &link).await.is_err());
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("not-a-real-file"));
        assert!(!dir.path().join("not-a-real-file").exists());
    }

    #[tokio::test]
    async fn corrupt_or_non_pos_source_is_rejected_without_publishing_output() {
        let dir = tempdir().unwrap();
        let corrupted = dir.path().join("corrupt.sqlite3");
        let output = dir.path().join("output.sqlite3");
        fs::write(&corrupted, b"not a SQLite file").unwrap();
        assert!(SqliteMaintenance::check(&corrupted).await.is_err());
        assert!(
            SqliteMaintenance::restore(&corrupted, &output)
                .await
                .is_err()
        );
        assert!(
            SqliteMaintenance::backup(&corrupted, &output)
                .await
                .is_err()
        );
        assert!(!output.exists());

        let other = dir.path().join("other.sqlite3");
        let options = SqliteConnectOptions::new()
            .filename(&other)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE unrelated (value TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert!(SqliteMaintenance::check(&other).await.is_err());
    }

    #[tokio::test]
    async fn foreign_key_violation_fails_integrity_check() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("fk.sqlite3");
        let store = SqliteTaskRepository::open(&live).await.unwrap();
        store
            .create(NewTask::new("Task", None).unwrap())
            .await
            .unwrap();

        let options = SqliteConnectOptions::new()
            .filename(&live)
            .foreign_keys(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("UPDATE tasks SET project_id = 999 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let result = SqliteMaintenance::check(&live).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("foreign-key violations")
        );
    }

    #[tokio::test]
    async fn empty_pos_database_is_valid_backup() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("empty.sqlite3");
        let store = SqliteTaskRepository::open(&live).await.unwrap();
        let row = sqlx::query("SELECT count(*) AS total FROM tasks")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(row.try_get::<i64, _>("total").unwrap(), 0);
        SqliteMaintenance::check(&live).await.unwrap();
        let backup = dir.path().join("snapshot.sqlite3");
        SqliteMaintenance::backup(&live, &backup).await.unwrap();
        assert!(backup.exists());
    }
}
