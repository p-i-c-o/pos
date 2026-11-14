//! PR #19 identity/binding tests, entirely disposable local SQLite.
use pos_entities::LocalRecordId;
use pos_projects::NewProject;
use pos_sqlite::{
    BindOutcome, RecordMapping, SqliteMaintenance, SqliteTaskRepository, SyncBinding,
};
use pos_storage::{ProjectRepository, TaskRepository};
use pos_sync::RecordKind;
use pos_tasks::NewTask;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tempfile::tempdir;
use uuid::Uuid;

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap()
}

fn binding(db: Uuid) -> SyncBinding {
    SyncBinding {
        database_id: db,
        server_origin: "http://127.0.0.1:8787/".into(),
        user_id: uuid("ac764621-c770-4bf4-89ee-25e1234dec99"),
        device_id: uuid("6831fd40-f38c-4a98-9cde-658567b38b67"),
        workspace_id: uuid("ed91a5ab-f2f7-4c23-a69a-c512e9667cd5"),
        canonical_path: "/tmp/test-disposable-pos-db.sqlite3".into(),
    }
}

#[tokio::test]
async fn database_id_is_stable_across_open_and_verified_backup_restore() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let project = repo
        .create_project(NewProject::new("Legacy", None).unwrap())
        .await
        .unwrap();
    let task = repo
        .create(NewTask::new("Keep", None).unwrap())
        .await
        .unwrap();
    let state = repo.sync_state().await.unwrap();
    assert_eq!(state.database_id.get_version_num(), 4);
    assert!(state.binding.is_none());
    assert_eq!(state.mapped_records, 0);
    assert_eq!(state.journal_entries, 2);
    let task_id = repo.task_sync_id(task.id).await.unwrap();
    let project_id = repo.project_sync_id(project.id).await.unwrap();
    drop(repo);

    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    assert_eq!(repo.sync_state().await.unwrap(), state);
    assert_eq!(repo.task_sync_id(task.id).await.unwrap(), task_id);
    assert_eq!(repo.project_sync_id(project.id).await.unwrap(), project_id);
    let backup = dir.path().join("backup.sqlite3");
    let restored = dir.path().join("restore.sqlite3");
    SqliteMaintenance::backup(&path, &backup).await.unwrap();
    SqliteMaintenance::restore(&backup, &restored)
        .await
        .unwrap();
    let copy = SqliteTaskRepository::open(&restored).await.unwrap();
    assert_eq!(copy.sync_state().await.unwrap(), state);
    assert_eq!(copy.task_sync_id(task.id).await.unwrap(), task_id);
    assert_eq!(copy.project_sync_id(project.id).await.unwrap(), project_id);
    assert_eq!(copy.get(task.id).await.unwrap().unwrap().title, "Keep");
}

#[tokio::test]
async fn workspace_bind_is_single_immutable_and_requires_correct_database_id() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let original = repo.sync_state().await.unwrap();
    let mut requested = binding(original.database_id);
    let mut wrong_id = requested.clone();
    wrong_id.database_id = uuid("0e4d660d-e2a5-4bb9-af4c-d2aa404c7a4d");
    assert!(repo.bind_workspace_once(&wrong_id).await.is_err());
    assert!(repo.sync_state().await.unwrap().binding.is_none());
    assert_eq!(
        repo.bind_workspace_once(&requested).await.unwrap(),
        BindOutcome::Created
    );
    assert_eq!(
        repo.bind_workspace_once(&requested).await.unwrap(),
        BindOutcome::AlreadyBound
    );
    assert_eq!(
        repo.sync_state().await.unwrap().binding,
        Some(requested.clone())
    );

    requested.workspace_id = uuid("ff443322-4455-4d77-aabb-998877665544");
    assert!(repo.bind_workspace_once(&requested).await.is_err());
    requested = binding(original.database_id);
    requested.device_id = uuid("9f38cbf0-a93d-4b40-bfff-26aa18772055");
    assert!(repo.bind_workspace_once(&requested).await.is_err());
    assert_eq!(
        repo.sync_state()
            .await
            .unwrap()
            .binding
            .unwrap()
            .workspace_id,
        binding(original.database_id).workspace_id
    );
    drop(repo);

    // Even direct SQL cannot silently rebind or delete immutable metadata.
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE pos_sync_binding SET workspace_id = 'wrong' WHERE slot = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM pos_sync_binding WHERE slot = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE pos_sync_database SET database_id = 'wrong' WHERE slot = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM pos_sync_database WHERE slot = 1")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn record_maps_are_unique_persistent_and_version_cas_is_monotonic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let repo = SqliteTaskRepository::open(&path).await.unwrap();
    let task = repo
        .create(NewTask::new("Offline", None).unwrap())
        .await
        .unwrap();
    let other = repo
        .create(NewTask::new("Second", None).unwrap())
        .await
        .unwrap();
    let local = repo.task_sync_id(task.id).await.unwrap().unwrap();
    let other_local = repo.task_sync_id(other.id).await.unwrap().unwrap();
    let remote = uuid("ca5bdbca-c711-44f9-8b00-ef2ed4bab057");
    let record = RecordMapping {
        kind: RecordKind::Task,
        local_id: local,
        server_id: remote,
        server_version: 1,
    };
    assert!(
        repo.record_mapping_once(&record).await.is_err(),
        "unbound mapping prohibited by FK"
    );
    repo.bind_workspace_once(&binding(repo.sync_state().await.unwrap().database_id))
        .await
        .unwrap();

    let invalid = RecordMapping {
        local_id: LocalRecordId::parse("cc845efe-b166-4b20-b4b5-0253f4dd3311").unwrap(),
        ..record.clone()
    };
    assert!(
        repo.record_mapping_once(&invalid).await.is_err(),
        "cannot invent local records"
    );
    let invalid_version = RecordMapping {
        server_version: 0,
        ..record.clone()
    };
    assert!(repo.record_mapping_once(&invalid_version).await.is_err());
    repo.record_mapping_once(&record).await.unwrap();
    assert_eq!(
        repo.record_mapping(RecordKind::Task, local).await.unwrap(),
        Some(record.clone())
    );
    assert!(
        repo.record_mapping_once(&record).await.is_err(),
        "never remap a local record"
    );
    let other_map = RecordMapping {
        local_id: other_local,
        ..record.clone()
    };
    assert!(
        repo.record_mapping_once(&other_map).await.is_err(),
        "one server UUID cannot map to two Tasks"
    );

    assert!(
        repo.advance_mapping_version(RecordKind::Task, local, 2, 3)
            .await
            .is_err()
    );
    assert!(
        repo.advance_mapping_version(RecordKind::Task, local, 1, 1)
            .await
            .is_err()
    );
    assert!(
        repo.advance_mapping_version(RecordKind::Task, local, 1, 0)
            .await
            .is_err()
    );
    repo.advance_mapping_version(RecordKind::Task, local, 1, 2)
        .await
        .unwrap();
    assert_eq!(
        repo.record_mapping(RecordKind::Task, local)
            .await
            .unwrap()
            .unwrap()
            .server_version,
        2
    );
    assert!(
        repo.advance_mapping_version(RecordKind::Task, local, 1, 3)
            .await
            .is_err()
    );

    // Tombstones retain the original mapping, including its server version.
    repo.delete(task.id).await.unwrap();
    assert_eq!(
        repo.record_mapping(RecordKind::Task, local)
            .await
            .unwrap()
            .unwrap()
            .server_id,
        remote
    );
    let mapped_state = repo.sync_state().await.unwrap();
    assert_eq!(mapped_state.mapped_records, 1);
    drop(repo);

    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    assert!(
        sqlx::query("DELETE FROM pos_sync_record_map")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE pos_sync_record_map SET server_record_id = 'wrong'")
            .execute(&pool)
            .await
            .is_err()
    );
    drop(pool);
    let reopened = SqliteTaskRepository::open(&path).await.unwrap();
    assert_eq!(reopened.sync_state().await.unwrap(), mapped_state);
    assert_eq!(
        reopened
            .record_mapping(RecordKind::Task, local)
            .await
            .unwrap()
            .unwrap()
            .server_version,
        2
    );
}

#[tokio::test]
async fn pr18_database_upgrades_once_without_changing_journal_or_task_ids() {
    let dir = tempdir().unwrap();
    let old = dir.path().join("pr18.sqlite3");
    let migrations = tempdir().unwrap();
    for (name, sql) in [
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
        (
            "20261114000000_sync_change_journal.sql",
            include_str!("../migrations/20261114000000_sync_change_journal.sql"),
        ),
    ] {
        std::fs::write(migrations.path().join(name), sql).unwrap();
    }
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&old)
            .create_if_missing(true)
            .foreign_keys(true),
    )
    .await
    .unwrap();
    sqlx::migrate::Migrator::new(migrations.path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO projects(name,slug) VALUES('Old','old')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tasks(title,project_id) VALUES('Old task',1)")
        .execute(&pool)
        .await
        .unwrap();
    let old_journal: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_sync_outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_journal, 2);
    let old_local_id: String = sqlx::query_scalar("SELECT sync_id FROM tasks WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    drop(pool);
    let db = SqliteTaskRepository::open(&old).await.unwrap();
    let state = db.sync_state().await.unwrap();
    assert!(state.binding.is_none());
    assert_eq!(state.journal_entries, old_journal);
    assert_eq!(state.mapped_records, 0);
    assert_eq!(
        db.task_sync_id(1).await.unwrap().unwrap().to_string(),
        old_local_id
    );
    drop(db);
    let db = SqliteTaskRepository::open(&old).await.unwrap();
    assert_eq!(db.sync_state().await.unwrap(), state);
    assert_eq!(
        db.task_sync_id(1).await.unwrap().unwrap().to_string(),
        old_local_id
    );
}
