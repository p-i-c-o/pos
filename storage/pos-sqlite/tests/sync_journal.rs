//! Durable journal tests use disposable SQLite only. No network or user data.
use pos_entities::LocalRecordId;
use pos_projects::NewProject;
use pos_sqlite::SqliteTaskRepository;
use pos_storage::{ProjectRepository, TaskRepository};
use pos_sync::{ChangeOperation, RecordKind};
use pos_tasks::{NewTask, TaskPatch};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tempfile::tempdir;

#[tokio::test]
async fn offline_writes_are_immutable_ordered_and_survive_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("durable.sqlite3");
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let project = repo
        .create_project(NewProject::new("University", None).unwrap())
        .await
        .unwrap();
    let task = repo
        .create(NewTask::new("First title", None).unwrap())
        .await
        .unwrap();
    let project_uid = repo.project_sync_id(project.id).await.unwrap().unwrap();
    let task_uid = repo.task_sync_id(task.id).await.unwrap().unwrap();
    assert_ne!(project_uid, task_uid);
    assert_eq!(project_uid.as_uuid().get_version_num(), 4);
    assert_eq!(task_uid.as_uuid().get_version_num(), 4);

    let first = repo.pending_sync_changes(0, 100).await.unwrap();
    assert_eq!(first.len(), 2, "one snapshot per INSERT");
    assert!(first.iter().all(|change| change.format_version == 1));
    assert_eq!(first[0].kind, RecordKind::Project);
    assert_eq!(first[1].kind, RecordKind::Task);
    assert_eq!(first[0].sync_id, project_uid);
    assert_eq!(first[1].sync_id, task_uid);
    assert_eq!(first[1].payload.as_ref().unwrap()["title"], "First title");
    assert!(first[1].payload.as_ref().unwrap()["project_sync_id"].is_null());

    repo.assign_project(task.id, Some(project.id))
        .await
        .unwrap();
    repo.update(task.id, TaskPatch::new(Some("Second title"), None).unwrap())
        .await
        .unwrap();
    repo.set_completed(task.id, true).await.unwrap();
    let changes = repo.pending_sync_changes(0, 100).await.unwrap();
    assert_eq!(changes.len(), 5);
    assert_eq!(
        changes[2].payload.as_ref().unwrap()["project_sync_id"],
        project_uid.to_string()
    );
    assert_eq!(
        changes[3].payload.as_ref().unwrap()["title"],
        "Second title"
    );
    assert_eq!(changes[4].payload.as_ref().unwrap()["completed"], true);
    assert_eq!(
        changes[1].payload.as_ref().unwrap()["title"],
        "First title",
        "history must not mutate"
    );

    assert_eq!(repo.task_sync_id(task.id).await.unwrap(), Some(task_uid));
    assert_eq!(
        repo.pending_sync_changes(changes[2].seq, 100)
            .await
            .unwrap(),
        changes[3..].to_vec()
    );
    assert_eq!(repo.pending_sync_changes(0, 1).await.unwrap().len(), 1);
    assert!(repo.pending_sync_changes(-1, 10).await.is_err());
    assert!(repo.pending_sync_changes(0, 0).await.is_err());
    assert!(repo.pending_sync_changes(0, 1001).await.is_err());

    drop(repo);
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    assert_eq!(
        repo.project_sync_id(project.id).await.unwrap(),
        Some(project_uid)
    );
    assert_eq!(repo.task_sync_id(task.id).await.unwrap(), Some(task_uid));
    assert_eq!(repo.pending_sync_changes(0, 100).await.unwrap(), changes);
}

#[tokio::test]
async fn failures_and_direct_sql_cannot_bypass_atomic_journaling() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("external.sqlite3");
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let task = repo
        .create(NewTask::new("Valid", None).unwrap())
        .await
        .unwrap();
    let uid = repo.task_sync_id(task.id).await.unwrap().unwrap();
    let original = repo.pending_sync_changes(0, 100).await.unwrap();
    assert_eq!(original.len(), 1);
    assert!(repo.assign_project(task.id, Some(9999)).await.is_err());
    assert_eq!(repo.pending_sync_changes(0, 100).await.unwrap(), original);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO tasks(title) VALUES ('Direct writer')")
        .execute(&pool)
        .await
        .unwrap();
    let direct_uid: String =
        sqlx::query_scalar("SELECT sync_id FROM tasks WHERE title = 'Direct writer'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(LocalRecordId::parse(&direct_uid).is_ok());
    assert!(
        sqlx::query("INSERT INTO tasks(title, sync_id) VALUES ('Bad UUID', 'not-a-uuid')")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO tasks(title, sync_id) VALUES ('Bad version', 'ff98ff98-aaaa-5abc-8000-123456789012')")
        .execute(&pool).await.is_err());
    sqlx::query("UPDATE tasks SET title = 'Externally edited' WHERE id = ?")
        .bind(task.id)
        .execute(&pool)
        .await
        .unwrap();

    let before = repo.pending_sync_changes(0, 100).await.unwrap().len();
    assert!(
        sqlx::query("UPDATE tasks SET title = '' WHERE id = ?")
            .bind(task.id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE tasks SET sync_id = 'different' WHERE id = ?")
            .bind(task.id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(
        repo.pending_sync_changes(0, 100).await.unwrap().len(),
        before
    );
    // A rolled-back transaction must roll back the journal event too.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE tasks SET title = 'Rolled back' WHERE id = ?")
        .bind(task.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        repo.pending_sync_changes(0, 100).await.unwrap().len(),
        before
    );
    assert_eq!(
        repo.get(task.id).await.unwrap().unwrap().title,
        "Externally edited"
    );
    assert!(
        sqlx::query("DELETE FROM pos_sync_outbox WHERE seq = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE pos_sync_outbox SET operation = 'delete' WHERE seq = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(repo.delete(task.id).await.unwrap());
    let events = repo.pending_sync_changes(0, 100).await.unwrap();
    assert_eq!(events.last().unwrap().sync_id, uid);
    assert_eq!(events.last().unwrap().operation, ChangeOperation::Delete);
    assert_eq!(events.last().unwrap().payload, None);
    assert_eq!(repo.task_sync_id(task.id).await.unwrap(), None);
    drop(pool);
    drop(repo);
    let reopened = SqliteTaskRepository::open(&path).await.unwrap();
    assert_eq!(reopened.pending_sync_changes(0, 100).await.unwrap(), events);
}

#[tokio::test]
async fn project_delete_records_unassignment_and_tombstone() {
    let dir = tempdir().unwrap();
    let repo = SqliteTaskRepository::open(&dir.path().join("project-delete.sqlite3"))
        .await
        .unwrap();
    let project = repo
        .create_project(NewProject::new("Temporary", None).unwrap())
        .await
        .unwrap();
    let task = repo
        .create(NewTask::new("Still here", None).unwrap())
        .await
        .unwrap();
    repo.assign_project(task.id, Some(project.id))
        .await
        .unwrap();
    let project_uid = repo.project_sync_id(project.id).await.unwrap().unwrap();
    let task_uid = repo.task_sync_id(task.id).await.unwrap().unwrap();
    let before = repo.pending_sync_changes(0, 100).await.unwrap().len();
    assert!(repo.delete_project(project.id).await.unwrap());
    assert_eq!(repo.get(task.id).await.unwrap().unwrap().project_id, None);
    let changes = repo.pending_sync_changes(0, 100).await.unwrap();
    let after = &changes[before..];
    assert!(after.iter().any(|change| change.kind == RecordKind::Project
        && change.sync_id == project_uid
        && change.operation == ChangeOperation::Delete));
    assert!(after.iter().any(|change| {
        change.kind == RecordKind::Task
            && change.sync_id == task_uid
            && change.operation == ChangeOperation::Upsert
            && change
                .payload
                .as_ref()
                .is_some_and(|json| json["project_sync_id"].is_null())
    }));
}

#[tokio::test]
async fn old_database_is_backfilled_once_and_keeps_unrelated_data() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("old.sqlite3");
    let migration_dir = tempdir().unwrap();
    for (name, text) in [
        (
            "20261112000000_create_tasks.sql",
            include_str!("../migrations/20261112000000_create_tasks.sql"),
        ),
        (
            "20261112010000_create_projects_and_task_links.sql",
            include_str!("../migrations/20261112010000_create_projects_and_task_links.sql"),
        ),
        (
            "20261112020000_task_engine_v2.sql",
            include_str!("../migrations/20261112020000_task_engine_v2.sql"),
        ),
    ] {
        std::fs::write(migration_dir.path().join(name), text).unwrap();
    }
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true),
    )
    .await
    .unwrap();
    sqlx::migrate::Migrator::new(migration_dir.path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE unrelated (label TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO unrelated(label) VALUES ('preserve')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO projects (name,slug) VALUES ('Old project','old-project')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tasks(title,project_id,completed,status) VALUES ('Old task',1,1,'completed')",
    )
    .execute(&pool)
    .await
    .unwrap();
    drop(pool);
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let project_uid = repo.project_sync_id(1).await.unwrap().unwrap();
    let task_uid = repo.task_sync_id(1).await.unwrap().unwrap();
    let changes = repo.pending_sync_changes(0, 100).await.unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].sync_id, project_uid);
    assert_eq!(changes[1].sync_id, task_uid);
    assert_eq!(
        changes[1].payload.as_ref().unwrap()["project_sync_id"],
        project_uid.to_string()
    );
    assert_eq!(changes[1].payload.as_ref().unwrap()["completed"], true);
    assert_eq!(repo.get(1).await.unwrap().unwrap().title, "Old task");
    assert_eq!(repo.get(1).await.unwrap().unwrap().project_id, Some(1));
    drop(repo);

    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    assert_eq!(repo.pending_sync_changes(0, 100).await.unwrap(), changes);
    assert_eq!(repo.task_sync_id(1).await.unwrap(), Some(task_uid));
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    let label: String = sqlx::query_scalar("SELECT label FROM unrelated")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(label, "preserve");
}

#[tokio::test]
async fn backup_and_restore_preserve_pending_history_and_identity() {
    use pos_sqlite::SqliteMaintenance;

    let dir = tempdir().unwrap();
    let original = dir.path().join("original.sqlite3");
    let backup = dir.path().join("backup.sqlite3");
    let restored = dir.path().join("restored.sqlite3");
    let repo = SqliteTaskRepository::open(&original).await.unwrap();
    let task = repo
        .create(NewTask::new("Protect this", None).unwrap())
        .await
        .unwrap();
    repo.update(
        task.id,
        TaskPatch::new(Some("Keep both versions"), None).unwrap(),
    )
    .await
    .unwrap();
    let identity = repo.task_sync_id(task.id).await.unwrap();
    let history = repo.pending_sync_changes(0, 100).await.unwrap();

    SqliteMaintenance::backup(&original, &backup).await.unwrap();
    SqliteMaintenance::restore(&backup, &restored)
        .await
        .unwrap();
    let recovered = SqliteTaskRepository::open(&restored).await.unwrap();
    assert_eq!(recovered.task_sync_id(task.id).await.unwrap(), identity);
    assert_eq!(
        recovered.pending_sync_changes(0, 100).await.unwrap(),
        history
    );
    assert_eq!(
        recovered.get(task.id).await.unwrap().unwrap().title,
        "Keep both versions"
    );
    assert_eq!(repo.pending_sync_changes(0, 100).await.unwrap(), history);
}
