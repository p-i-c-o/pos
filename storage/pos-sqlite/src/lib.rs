//! SQLite implementation of POS task persistence using SQLx.

use pos_projects::{NewProject, Project};
use pos_storage::{
    LocalSnapshot, ProjectRepository, SnapshotRepository, StorageError, TaskRepository,
};
use pos_tasks::{NewTask, Task, TaskPatch, TaskPriority, TaskQuery, TaskSort, TaskStatus};
use sqlx::{
    QueryBuilder, Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use std::path::Path;

mod maintenance;
pub use maintenance::SqliteMaintenance;

#[derive(Clone)]
pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
    /// Open a validated existing POS database read-only, without creating files
    /// or applying SQLx migrations. Used for JSON export and inspection.
    pub async fn open_read_only(path: &Path) -> Result<Self, StorageError> {
        SqliteMaintenance::check(path).await?;
        let pool = maintenance::read_only_pool(path).await?;
        Ok(Self { pool })
    }

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
    let task = Task {
        id: row.try_get("id").map_err(storage_err)?,
        title: row.try_get("title").map_err(storage_err)?,
        description: row.try_get("description").map_err(storage_err)?,
        priority: row
            .try_get::<String, _>("priority")
            .map_err(storage_err)?
            .parse::<TaskPriority>()
            .map_err(storage_err)?,
        status: row
            .try_get::<String, _>("status")
            .map_err(storage_err)?
            .parse::<TaskStatus>()
            .map_err(storage_err)?,
        due_date: row.try_get("due_date").map_err(storage_err)?,
        project_id: row.try_get("project_id").map_err(storage_err)?,
        completed: row.try_get::<i64, _>("completed").map_err(storage_err)? != 0,
        created_at: row.try_get("created_at").map_err(storage_err)?,
        updated_at: row.try_get("updated_at").map_err(storage_err)?,
    };
    if task.completed != (task.status == TaskStatus::Completed) {
        return Err(StorageError(format!(
            "task #{} has inconsistent completion status",
            task.id
        )));
    }
    Ok(task)
}

const TASK_COLUMNS: &str = "id, title, description, priority, status, due_date, project_id, completed, created_at, updated_at";

impl TaskRepository for SqliteTaskRepository {
    async fn create(&self, task: NewTask) -> Result<Task, StorageError> {
        let id = sqlx::query(
            "INSERT INTO tasks (title, due_date, project_id, description, priority, status, completed)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&task.title)
            .bind(&task.due_date)
            .bind(task.project_id)
            .bind(&task.description)
            .bind(task.priority.as_str())
            .bind(task.status.as_str())
            .bind(task.status == TaskStatus::Completed)
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

    async fn query_tasks(&self, query: TaskQuery) -> Result<Vec<Task>, StorageError> {
        // All untrusted user inputs are bound parameters. Only fixed, known
        // ORDER BY expressions are selected based on a validated enum.
        let mut statement =
            QueryBuilder::<Sqlite>::new(format!("SELECT {TASK_COLUMNS} FROM tasks WHERE 1=1"));
        if !query.include_completed {
            statement.push(" AND completed = 0");
        }
        if let Some(project_id) = query.project_id {
            statement.push(" AND project_id = ").push_bind(project_id);
        }
        if let Some(priority) = query.priority {
            statement
                .push(" AND priority = ")
                .push_bind(priority.as_str());
        }
        if let Some(status) = query.status {
            statement.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(due_before) = query.due_before {
            statement.push(" AND due_date <= ").push_bind(due_before);
        }
        if let Some(search) = query.search {
            // instr() treats '%' and '_' literally, unlike a LIKE pattern.
            statement
                .push(" AND (instr(lower(title), lower(")
                .push_bind(search.clone())
                .push(")) > 0 OR instr(lower(coalesce(description, '')), lower(")
                .push_bind(search)
                .push(")) > 0)");
        }

        statement.push(" ORDER BY completed ASC, ");
        match query.sort {
            TaskSort::Due => {
                statement.push("due_date IS NULL ASC, due_date ASC, id ASC");
            }
            TaskSort::Priority => {
                statement.push(
                    "CASE priority WHEN 'urgent' THEN 4 WHEN 'high' THEN 3
                     WHEN 'normal' THEN 2 ELSE 1 END DESC,
                     due_date IS NULL ASC, due_date ASC, id ASC",
                );
            }
            TaskSort::Created => {
                statement.push("created_at DESC, id DESC");
            }
        }
        statement
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_task)
            .collect()
    }

    async fn list_for_project(
        &self,
        include_completed: bool,
        project_id: i64,
    ) -> Result<Vec<Task>, StorageError> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM tasks
             WHERE project_id = ? AND (? = 1 OR completed = 0)
             ORDER BY completed ASC, due_date IS NULL ASC, due_date ASC, id ASC"
        );
        sqlx::query(&sql)
            .bind(project_id)
            .bind(include_completed)
            .fetch_all(&self.pool)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_task)
            .collect()
    }

    async fn assign_project(
        &self,
        task_id: i64,
        project_id: Option<i64>,
    ) -> Result<Option<Task>, StorageError> {
        let affected = sqlx::query(
            "UPDATE tasks SET project_id = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(project_id)
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if affected == 0 {
            Ok(None)
        } else {
            self.get(task_id).await
        }
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
        let has_description = patch.description.is_some();
        let has_priority = patch.priority.is_some();
        let has_status = patch.status.is_some();
        let has_project = patch.project_id.is_some();
        let due_date = patch.due_date.flatten();
        let description = patch.description.flatten();
        let priority = patch.priority.map(TaskPriority::as_str);
        let status = patch.status.map(TaskStatus::as_str);
        let completed = patch.status == Some(TaskStatus::Completed);
        let project_id = patch.project_id.flatten();
        let changed = sqlx::query(
            "UPDATE tasks SET
                title = CASE WHEN ? THEN ? ELSE title END,
                due_date = CASE WHEN ? THEN ? ELSE due_date END,
                description = CASE WHEN ? THEN ? ELSE description END,
                priority = CASE WHEN ? THEN ? ELSE priority END,
                status = CASE WHEN ? THEN ? ELSE status END,
                completed = CASE WHEN ? THEN ? ELSE completed END,
                project_id = CASE WHEN ? THEN ? ELSE project_id END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?",
        )
        .bind(has_title)
        .bind(patch.title)
        .bind(has_due_date)
        .bind(due_date)
        .bind(has_description)
        .bind(description)
        .bind(has_priority)
        .bind(priority)
        .bind(has_status)
        .bind(status)
        .bind(has_status)
        .bind(completed)
        .bind(has_project)
        .bind(project_id)
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
            "UPDATE tasks SET completed = ?, status = ?,
              updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(completed)
        .bind(if completed {
            TaskStatus::Completed.as_str()
        } else {
            TaskStatus::Todo.as_str()
        })
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

fn row_to_project(row: SqliteRow) -> Result<Project, StorageError> {
    Ok(Project {
        id: row.try_get("id").map_err(storage_err)?,
        name: row.try_get("name").map_err(storage_err)?,
        slug: row.try_get("slug").map_err(storage_err)?,
        archived: row.try_get::<i64, _>("archived").map_err(storage_err)? != 0,
        created_at: row.try_get("created_at").map_err(storage_err)?,
        updated_at: row.try_get("updated_at").map_err(storage_err)?,
    })
}

const PROJECT_COLUMNS: &str = "id, name, slug, archived, created_at, updated_at";

impl ProjectRepository for SqliteTaskRepository {
    async fn create_project(&self, project: NewProject) -> Result<Project, StorageError> {
        let id = sqlx::query("INSERT INTO projects (name, slug) VALUES (?, ?)")
            .bind(project.name)
            .bind(project.slug)
            .execute(&self.pool)
            .await
            .map_err(storage_err)?
            .last_insert_rowid();
        self.get_project(id)
            .await?
            .ok_or_else(|| StorageError("inserted project missing".into()))
    }

    async fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>, StorageError> {
        let sql = format!(
            "SELECT {PROJECT_COLUMNS} FROM projects WHERE (? = 1 OR archived = 0)
             ORDER BY archived ASC, name COLLATE NOCASE ASC, id ASC"
        );
        sqlx::query(&sql)
            .bind(include_archived)
            .fetch_all(&self.pool)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_project)
            .collect()
    }

    async fn get_project(&self, id: i64) -> Result<Option<Project>, StorageError> {
        let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?");
        sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_err)?
            .map(row_to_project)
            .transpose()
    }

    async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>, StorageError> {
        let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE slug = ?");
        sqlx::query(&sql)
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_err)?
            .map(row_to_project)
            .transpose()
    }

    async fn rename_project(&self, id: i64, name: String) -> Result<Option<Project>, StorageError> {
        let affected = sqlx::query(
            "UPDATE projects SET name = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(name)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if affected == 0 {
            Ok(None)
        } else {
            self.get_project(id).await
        }
    }

    async fn archive_project(
        &self,
        id: i64,
        archived: bool,
    ) -> Result<Option<Project>, StorageError> {
        let affected = sqlx::query(
            "UPDATE projects SET archived = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(archived)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if affected == 0 {
            Ok(None)
        } else {
            self.get_project(id).await
        }
    }

    async fn delete_project(&self, id: i64) -> Result<bool, StorageError> {
        Ok(sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(storage_err)?
            .rows_affected()
            > 0)
    }
}

impl SnapshotRepository for SqliteTaskRepository {
    async fn snapshot(&self) -> Result<LocalSnapshot, StorageError> {
        // A single read transaction sees one consistent SQLite snapshot even if
        // another connection writes tasks/projects between the two SELECTs.
        let mut tx = self.pool.begin().await.map_err(storage_err)?;
        let tasks_sql = format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY id");
        let tasks: Vec<Task> = sqlx::query(&tasks_sql)
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_task)
            .collect::<Result<_, _>>()?;

        let projects_sql = format!("SELECT {PROJECT_COLUMNS} FROM projects ORDER BY id");
        let projects: Vec<Project> = sqlx::query(&projects_sql)
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_err)?
            .into_iter()
            .map(row_to_project)
            .collect::<Result<_, _>>()?;
        tx.commit().await.map_err(storage_err)?;
        Ok(LocalSnapshot { tasks, projects })
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
    async fn migration_preserves_phase3_tasks_and_unrelated_tables() {
        use sqlx::sqlite::SqliteConnectOptions;

        let dir = tempdir().unwrap();
        let path = dir.path().join("phase3.sqlite3");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        // Simulate the v0.1 Tasks-only schema before the Projects migration.
        sqlx::raw_sql(include_str!(
            "../migrations/20261112000000_create_tasks.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks(title, due_date, completed) VALUES ('Legacy task', '2026-12-01', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE unrelated (value TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO unrelated(value) VALUES ('preserve')")
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        let store = SqliteTaskRepository::open(&path).await.unwrap();
        let old = store.get(1).await.unwrap().unwrap();
        assert_eq!(old.title, "Legacy task");
        assert!(old.completed);
        assert_eq!(old.project_id, None);
        let preserved: String = sqlx::query_scalar("SELECT value FROM unrelated")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(preserved, "preserve");
        let project = store
            .create_project(NewProject::new("University", None).unwrap())
            .await
            .unwrap();
        store
            .assign_project(old.id, Some(project.id))
            .await
            .unwrap();
        assert_eq!(
            store.get(old.id).await.unwrap().unwrap().project_id,
            Some(project.id)
        );
        drop(store);

        let store = SqliteTaskRepository::open(&path).await.unwrap();
        assert_eq!(store.get(1).await.unwrap().unwrap().project_id, Some(1));
        assert_eq!(
            store.get_project(1).await.unwrap().unwrap().slug,
            "university"
        );
        store.delete_project(1).await.unwrap();
        assert_eq!(store.get(1).await.unwrap().unwrap().project_id, None);
        assert_eq!(store.list(true).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn foreign_key_rejects_nonexistent_project_assignment() {
        let dir = tempdir().unwrap();
        let store = SqliteTaskRepository::open(&dir.path().join("tasks.sqlite3"))
            .await
            .unwrap();
        let task = store
            .create(NewTask::new("Test", None).unwrap())
            .await
            .unwrap();
        assert!(store.assign_project(task.id, Some(999)).await.is_err());
        assert_eq!(store.get(task.id).await.unwrap().unwrap().project_id, None);
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

    #[tokio::test]
    async fn task_engine_queries_are_bound_sorted_and_consistent() {
        use pos_tasks::{TaskPriority, TaskQuery, TaskSort, TaskStatus};

        let dir = tempdir().unwrap();
        let store = SqliteTaskRepository::open(&dir.path().join("filter.sqlite3"))
            .await
            .unwrap();

        let normal = store
            .create(
                NewTask::with_details(
                    "Normal report",
                    Some("2026-11-28"),
                    None,
                    TaskPriority::Normal,
                    TaskStatus::Todo,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let urgent = store
            .create(
                NewTask::with_details(
                    "Urgent fix",
                    Some("2026-12-01"),
                    Some("needle 100% important"),
                    TaskPriority::Urgent,
                    TaskStatus::InProgress,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let high = store
            .create(
                NewTask::with_details(
                    "High _ task",
                    None,
                    None,
                    TaskPriority::High,
                    TaskStatus::Blocked,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let completed = store
            .create(
                NewTask::with_details(
                    "Done task",
                    None,
                    None,
                    TaskPriority::Low,
                    TaskStatus::Completed,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(completed.completed);

        let query =
            TaskQuery::new(false, None, None, None, None, None, TaskSort::Priority).unwrap();
        let ids: Vec<_> = store
            .query_tasks(query)
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![urgent.id, high.id, normal.id]);

        let query = TaskQuery::new(true, None, None, None, None, None, TaskSort::Priority).unwrap();
        let ids: Vec<_> = store
            .query_tasks(query)
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![urgent.id, high.id, normal.id, completed.id]);

        let query =
            TaskQuery::new(true, None, Some("100%"), None, None, None, TaskSort::Due).unwrap();
        assert_eq!(store.query_tasks(query).await.unwrap()[0].id, urgent.id);
        let query = TaskQuery::new(true, None, Some("_"), None, None, None, TaskSort::Due).unwrap();
        assert_eq!(store.query_tasks(query).await.unwrap()[0].id, high.id);
        let query = TaskQuery::new(
            true,
            None,
            Some("' OR 1=1 --"),
            None,
            None,
            None,
            TaskSort::Due,
        )
        .unwrap();
        assert!(store.query_tasks(query).await.unwrap().is_empty());

        let query = TaskQuery::new(
            false,
            None,
            None,
            None,
            None,
            Some("2026-11-28"),
            TaskSort::Due,
        )
        .unwrap();
        assert_eq!(store.query_tasks(query).await.unwrap()[0].id, normal.id);
        let query = TaskQuery::new(
            true,
            None,
            None,
            None,
            Some(TaskStatus::Completed),
            None,
            TaskSort::Due,
        )
        .unwrap();
        assert_eq!(store.query_tasks(query).await.unwrap()[0].id, completed.id);
        let query = TaskQuery::new(
            false,
            None,
            None,
            Some(TaskPriority::Urgent),
            None,
            None,
            TaskSort::Due,
        )
        .unwrap();
        assert_eq!(store.query_tasks(query).await.unwrap()[0].id, urgent.id);

        let edited = store
            .update(
                urgent.id,
                TaskPatch::full(
                    None,
                    None,
                    Some(None),
                    Some(TaskPriority::Low),
                    Some(TaskStatus::Blocked),
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(edited.description, None);
        assert_eq!(edited.status, TaskStatus::Blocked);
        assert_eq!(edited.priority, TaskPriority::Low);
        assert!(!edited.completed);
        let done = store.set_completed(urgent.id, true).await.unwrap().unwrap();
        assert_eq!(done.status, TaskStatus::Completed);
        let reopened = store
            .set_completed(urgent.id, false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reopened.status, TaskStatus::Todo);

        // The migration enforces consistency even when SQL is written directly.
        assert!(
            sqlx::query("UPDATE tasks SET completed = 1 WHERE id = ?")
                .bind(normal.id)
                .execute(&store.pool)
                .await
                .is_err()
        );
        assert_eq!(
            store.get(normal.id).await.unwrap().unwrap().status,
            TaskStatus::Todo
        );
    }

    #[tokio::test]
    async fn phase4_database_migrates_without_losing_completed_tasks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("phase4.sqlite3");
        let migrations = tempdir().unwrap();
        std::fs::write(
            migrations.path().join("20261112000000_create_tasks.sql"),
            include_str!("../migrations/20261112000000_create_tasks.sql"),
        )
        .unwrap();
        std::fs::write(
            migrations
                .path()
                .join("20261112010000_create_projects_and_task_links.sql"),
            include_str!("../migrations/20261112010000_create_projects_and_task_links.sql"),
        )
        .unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::migrate::Migrator::new(migrations.path())
            .await
            .unwrap()
            .run(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO projects(name,slug) VALUES ('Legacy project','legacy')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tasks(title,due_date,project_id,completed) VALUES ('Legacy completed','2026-12-01',1,1)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO tasks(title,completed) VALUES ('Legacy todo',0)")
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        let store = SqliteTaskRepository::open(&path).await.unwrap();
        let old = store.get(1).await.unwrap().unwrap();
        assert_eq!(old.title, "Legacy completed");
        assert_eq!(old.project_id, Some(1));
        assert_eq!(old.due_date.as_deref(), Some("2026-12-01"));
        assert_eq!(old.priority, TaskPriority::Normal);
        assert_eq!(old.status, TaskStatus::Completed);
        assert!(old.completed);
        assert_eq!(old.description, None);
        let todo = store.get(2).await.unwrap().unwrap();
        assert_eq!(todo.status, TaskStatus::Todo);
        assert!(!todo.completed);
        assert_eq!(store.get_project(1).await.unwrap().unwrap().slug, "legacy");

        // Migrations are repeatable and don't modify already-migrated data.
        drop(store);
        let store = SqliteTaskRepository::open(&path).await.unwrap();
        assert_eq!(
            store.get(1).await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
    }
}
