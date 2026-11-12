//! SQLite implementation of POS task persistence using SQLx.

use pos_storage::{StorageError, TaskRepository};
use pos_tasks::{NewTask, Task, TaskPatch};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use std::path::Path;

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
    /// Connect to an existing SQLite file or create one. Migrations only add
    /// POS-owned tables; existing application tables are not modified.
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(storage_err)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(storage_err)?;
        Ok(Self { pool })
    }
}

fn storage_err(error: impl std::fmt::Display) -> StorageError {
    StorageError(error.to_string())
}

fn row_to_task(row: SqliteRow) -> Result<Task, StorageError> {
    Ok(Task {
        id: row.try_get("id").map_err(storage_err)?,
        title: row.try_get("title").map_err(storage_err)?,
        due_date: row.try_get("due_date").map_err(storage_err)?,
        completed: row.try_get::<i64, _>("completed").map_err(storage_err)? != 0,
        created_at: row.try_get("created_at").map_err(storage_err)?,
        updated_at: row.try_get("updated_at").map_err(storage_err)?,
    })
}

const TASK_COLUMNS: &str = "id, title, due_date, completed, created_at, updated_at";

impl TaskRepository for SqliteTaskRepository {
    async fn create(&self, task: NewTask) -> Result<Task, StorageError> {
        let id = sqlx::query("INSERT INTO tasks (title, due_date) VALUES (?, ?)")
            .bind(&task.title)
            .bind(&task.due_date)
            .execute(&self.pool)
            .await
            .map_err(storage_err)?
            .last_insert_rowid();
        self.get(id)
            .await?
            .ok_or_else(|| StorageError("inserted task missing".into()))
    }

    async fn list(&self, include_completed: bool) -> Result<Vec<Task>, StorageError> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE (? = 1 OR completed = 0)              ORDER BY completed ASC, due_date IS NULL ASC, due_date ASC, id ASC"
        );
        sqlx::query(&sql)
            .bind(include_completed)
            .fetch_all(&self.pool)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_task)
            .collect()
    }

    async fn get(&self, id: i64) -> Result<Option<Task>, StorageError> {
        let sql = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?");
        sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_err)?
            .map(row_to_task)
            .transpose()
    }

    async fn update(&self, id: i64, patch: TaskPatch) -> Result<Option<Task>, StorageError> {
        // Changes are atomic. title and date are separately optional, and date can be cleared.
        let has_title = patch.title.is_some();
        let has_due_date = patch.due_date.is_some();
        let due_date = patch.due_date.flatten();
        let changed = sqlx::query(
            "UPDATE tasks SET
                title = CASE WHEN ? THEN ? ELSE title END,
                due_date = CASE WHEN ? THEN ? ELSE due_date END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?",
        )
        .bind(has_title)
        .bind(patch.title)
        .bind(has_due_date)
        .bind(due_date)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if changed == 0 {
            Ok(None)
        } else {
            self.get(id).await
        }
    }

    async fn set_completed(&self, id: i64, completed: bool) -> Result<Option<Task>, StorageError> {
        let changed = sqlx::query(
            "UPDATE tasks SET completed = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(completed)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if changed == 0 {
            Ok(None)
        } else {
            self.get(id).await
        }
    }

    async fn delete(&self, id: i64) -> Result<bool, StorageError> {
        Ok(sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(storage_err)?
            .rows_affected()
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_tasks::TaskPatch;
    use tempfile::tempdir;

    #[tokio::test]
    async fn create_list_edit_complete_reopen_delete_and_reopen_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite3");
        let store = SqliteTaskRepository::open(&path).await.unwrap();
        let task = store
            .create(NewTask::new("first", Some("2026-11-30")).unwrap())
            .await
            .unwrap();
        assert_eq!(store.list(false).await.unwrap().len(), 1);
        let task = store
            .update(
                task.id,
                TaskPatch::new(Some("renamed"), Some(None)).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.title, "renamed");
        assert_eq!(task.due_date, None);
        store.set_completed(task.id, true).await.unwrap();
        assert!(store.list(false).await.unwrap().is_empty());
        drop(store);

        let store = SqliteTaskRepository::open(&path).await.unwrap();
        assert!(store.get(task.id).await.unwrap().unwrap().completed);
        assert_eq!(store.list(true).await.unwrap().len(), 1);
        store.set_completed(task.id, false).await.unwrap();
        assert_eq!(store.list(false).await.unwrap().len(), 1);
        assert!(store.delete(task.id).await.unwrap());
        assert!(!store.delete(task.id).await.unwrap());
        assert!(store.get(task.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn opening_existing_database_preserves_foreign_tables() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.sqlite3");
        let store = SqliteTaskRepository::open(&path).await.unwrap();
        sqlx::query("CREATE TABLE unrelated (value TEXT)")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO unrelated(value) VALUES ('keep')")
            .execute(&store.pool)
            .await
            .unwrap();
        drop(store);
        let store = SqliteTaskRepository::open(&path).await.unwrap();
        let preserved: String = sqlx::query_scalar("SELECT value FROM unrelated LIMIT 1")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(preserved, "keep");
    }
}
