//! Workspace-scoped Projects/Tasks storage. All queries (including writes)
//! check active user and the workspace role inside PostgreSQL. Never accept
//! a raw workspace ID as sufficient authorization.
//!
//! Server IDs are UUIDs and are deliberately NOT interchangeable with
//! offline SQLite integer IDs. No automatic replication is attempted.

use std::{error::Error, fmt};

use pos_projects::{NewProject, validate_name};
use pos_tasks::{NewTask, TaskPatch, TaskPriority, TaskStatus};
use serde::Serialize;
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::PostgresStore;

const READ_SCOPE: &str = "EXISTS (
    SELECT 1 FROM pos_workspaces w
    JOIN pos_users actor ON actor.id = $1 AND actor.disabled_at IS NULL
    LEFT JOIN pos_workspace_members m ON m.workspace_id = w.id AND m.user_id = $1
    WHERE w.id = $2
      AND (w.owner_user_id = $1 OR (w.kind = 'shared' AND m.role IN ('viewer', 'editor')))
)";
const WRITE_SCOPE: &str = "EXISTS (
    SELECT 1 FROM pos_workspaces w
    JOIN pos_users actor ON actor.id = $1 AND actor.disabled_at IS NULL
    LEFT JOIN pos_workspace_members m ON m.workspace_id = w.id AND m.user_id = $1
    WHERE w.id = $2
      AND (w.owner_user_id = $1 OR (w.kind = 'shared' AND m.role = 'editor'))
)";

const PROJECT_FIELDS: &str = "id, workspace_id, name, slug, archived, version, \
    created_at::text AS created_at, updated_at::text AS updated_at";
const TASK_FIELDS: &str = "id, workspace_id, project_id, title, description, \
    priority, status, due_date::text AS due_date, version, \
    created_at::text AS created_at, updated_at::text AS updated_at";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    Invalid,
    NotFound,
    Conflict,
    Unavailable,
}
impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Invalid => "invalid record input",
            Self::NotFound => "record not found",
            Self::Conflict => "duplicate or stale record version",
            Self::Unavailable => "record storage unavailable",
        };
        f.write_str(message)
    }
}
impl Error for RecordError {}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub slug: String,
    pub archived: bool,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub priority: String,
    pub status: String,
    pub due_date: Option<String>,
    pub completed: bool,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Server-side task creation input, validated with the shared local domain.
#[derive(Clone, Debug)]
pub struct CreateTaskRecord {
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub due_date: Option<String>,
    pub project_id: Option<Uuid>,
}

/// Every field is optional. Nested Option distinguishes unchanged from
/// clearing nullable fields. Version is passed separately and mandatory.
#[derive(Clone, Debug, Default)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub priority: Option<TaskPriority>,
    pub status: Option<TaskStatus>,
    pub due_date: Option<Option<String>>,
    pub project_id: Option<Option<Uuid>>,
}

fn project_from(row: PgRow) -> Result<ProjectRecord, RecordError> {
    let err = |_| RecordError::Unavailable;
    Ok(ProjectRecord {
        id: row.try_get("id").map_err(err)?,
        workspace_id: row.try_get("workspace_id").map_err(err)?,
        name: row.try_get("name").map_err(err)?,
        slug: row.try_get("slug").map_err(err)?,
        archived: row.try_get("archived").map_err(err)?,
        version: row.try_get("version").map_err(err)?,
        created_at: row.try_get("created_at").map_err(err)?,
        updated_at: row.try_get("updated_at").map_err(err)?,
    })
}
fn task_from(row: PgRow) -> Result<TaskRecord, RecordError> {
    let err = |_| RecordError::Unavailable;
    let status: String = row.try_get("status").map_err(err)?;
    Ok(TaskRecord {
        id: row.try_get("id").map_err(err)?,
        workspace_id: row.try_get("workspace_id").map_err(err)?,
        project_id: row.try_get("project_id").map_err(err)?,
        title: row.try_get("title").map_err(err)?,
        description: row.try_get("description").map_err(err)?,
        priority: row.try_get("priority").map_err(err)?,
        completed: status == "completed",
        status,
        due_date: row.try_get("due_date").map_err(err)?,
        version: row.try_get("version").map_err(err)?,
        created_at: row.try_get("created_at").map_err(err)?,
        updated_at: row.try_get("updated_at").map_err(err)?,
    })
}

impl PostgresStore {
    /// Workspace authorization is evaluated at the database boundary.
    /// Use this for 404 on inaccessible collection paths, but read/write
    /// SQL also rechecks permission in its own statement.
    pub async fn records_allowed(
        &self,
        actor: Uuid,
        workspace: Uuid,
        write: bool,
    ) -> Result<bool, RecordError> {
        let scope = if write { WRITE_SCOPE } else { READ_SCOPE };
        sqlx::query_scalar::<_, bool>(&format!("SELECT {scope}"))
            .bind(actor)
            .bind(workspace)
            .fetch_one(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)
    }

    pub async fn create_project_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        name: &str,
        slug: Option<&str>,
    ) -> Result<ProjectRecord, RecordError> {
        let project = NewProject::new(name, slug).map_err(|_| RecordError::Invalid)?;
        let sql = format!(
            "INSERT INTO pos_projects (workspace_id, name, slug)
             SELECT $2, $3, $4 WHERE {WRITE_SCOPE}
             RETURNING {PROJECT_FIELDS}"
        );
        let result = sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(project.name)
            .bind(project.slug)
            .fetch_optional(&self.pool)
            .await;
        match result {
            Ok(Some(row)) => project_from(row),
            Ok(None) => Err(RecordError::NotFound),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(RecordError::Conflict),
            Err(_) => Err(RecordError::Unavailable),
        }
    }

    pub async fn list_project_records(
        &self,
        actor: Uuid,
        workspace: Uuid,
    ) -> Result<Vec<ProjectRecord>, RecordError> {
        if !self.records_allowed(actor, workspace, false).await? {
            return Err(RecordError::NotFound);
        }
        let sql = format!(
            "SELECT {PROJECT_FIELDS} FROM pos_projects p
             WHERE p.workspace_id = $2 AND {READ_SCOPE}
             ORDER BY p.created_at, p.id LIMIT 200"
        );
        sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?
            .into_iter()
            .map(project_from)
            .collect()
    }

    pub async fn project_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        id: Uuid,
    ) -> Result<Option<ProjectRecord>, RecordError> {
        let sql = format!(
            "SELECT {PROJECT_FIELDS} FROM pos_projects p
             WHERE p.workspace_id = $2 AND p.id = $3 AND {READ_SCOPE}"
        );
        sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?
            .map(project_from)
            .transpose()
    }

    pub async fn update_project_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        id: Uuid,
        expected_version: i64,
        name: Option<&str>,
        archived: Option<bool>,
    ) -> Result<ProjectRecord, RecordError> {
        if expected_version < 1 || (name.is_none() && archived.is_none()) {
            return Err(RecordError::Invalid);
        }
        let name = name
            .map(validate_name)
            .transpose()
            .map_err(|_| RecordError::Invalid)?;
        let sql = format!(
            "UPDATE pos_projects p
             SET name = COALESCE($5::text, p.name),
                 archived = COALESCE($6::bool, p.archived),
                 version = p.version + 1,
                 updated_at = clock_timestamp()
             WHERE p.workspace_id = $2 AND p.id = $3 AND p.version = $4
                   AND {WRITE_SCOPE}
             RETURNING {PROJECT_FIELDS}"
        );
        let row = sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(id)
            .bind(expected_version)
            .bind(name)
            .bind(archived)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?;
        if let Some(row) = row {
            return project_from(row);
        }
        // Do not reveal record existence to a user without WRITE permission.
        if !self.records_allowed(actor, workspace, true).await? {
            return Err(RecordError::NotFound);
        }
        if self.project_record(actor, workspace, id).await?.is_some() {
            Err(RecordError::Conflict)
        } else {
            Err(RecordError::NotFound)
        }
    }

    pub async fn create_task_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        input: &CreateTaskRecord,
    ) -> Result<TaskRecord, RecordError> {
        let task = NewTask::with_details(
            &input.title,
            input.due_date.as_deref(),
            input.description.as_deref(),
            input.priority,
            input.status,
        )
        .map_err(|_| RecordError::Invalid)?;
        let sql = format!(
            "INSERT INTO pos_tasks
             (workspace_id, project_id, title, description, priority, status, due_date)
             SELECT $2, $3::uuid, $4, $5, $6, $7, $8::date
             WHERE {WRITE_SCOPE}
               AND ($3::uuid IS NULL OR EXISTS
                   (SELECT 1 FROM pos_projects p WHERE p.id = $3
                    AND p.workspace_id = $2 AND NOT p.archived))
             RETURNING {TASK_FIELDS}"
        );
        let row = sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(input.project_id)
            .bind(task.title)
            .bind(task.description)
            .bind(task.priority.as_str())
            .bind(task.status.as_str())
            .bind(task.due_date)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?
            .ok_or(RecordError::NotFound)?;
        task_from(row)
    }

    pub async fn list_task_records(
        &self,
        actor: Uuid,
        workspace: Uuid,
    ) -> Result<Vec<TaskRecord>, RecordError> {
        if !self.records_allowed(actor, workspace, false).await? {
            return Err(RecordError::NotFound);
        }
        let sql = format!(
            "SELECT {TASK_FIELDS} FROM pos_tasks t
             WHERE t.workspace_id = $2 AND {READ_SCOPE}
             ORDER BY t.created_at, t.id LIMIT 200"
        );
        sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?
            .into_iter()
            .map(task_from)
            .collect()
    }

    pub async fn task_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        id: Uuid,
    ) -> Result<Option<TaskRecord>, RecordError> {
        let sql = format!(
            "SELECT {TASK_FIELDS} FROM pos_tasks t
             WHERE t.workspace_id = $2 AND t.id = $3 AND {READ_SCOPE}"
        );
        sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?
            .map(task_from)
            .transpose()
    }

    pub async fn update_task_record(
        &self,
        actor: Uuid,
        workspace: Uuid,
        id: Uuid,
        expected_version: i64,
        change: &TaskChanges,
    ) -> Result<TaskRecord, RecordError> {
        if expected_version < 1 {
            return Err(RecordError::Invalid);
        }
        // Reuse the same validation rules as the local SQLite Tasks model.
        // A project UUID is handled separately, but counts as an edit.
        let patch = TaskPatch::full(
            change.title.as_deref(),
            change.due_date.as_ref().map(|v| v.as_deref()),
            change.description.as_ref().map(|v| v.as_deref()),
            change.priority,
            change.status,
            change.project_id.map(|_| None),
        )
        .map_err(|_| RecordError::Invalid)?;

        let has_description = patch.description.is_some();
        let has_due = patch.due_date.is_some();
        let has_project = change.project_id.is_some();
        let sql = format!(
            "UPDATE pos_tasks t
             SET title = COALESCE($5::text, t.title),
                 description = CASE WHEN $6::bool THEN $7::text ELSE t.description END,
                 priority = COALESCE($8::text, t.priority),
                 status = COALESCE($9::text, t.status),
                 due_date = CASE WHEN $10::bool THEN $11::date ELSE t.due_date END,
                 project_id = CASE WHEN $12::bool THEN $13::uuid ELSE t.project_id END,
                 version = t.version + 1,
                 updated_at = clock_timestamp()
             WHERE t.workspace_id = $2 AND t.id = $3 AND t.version = $4
               AND {WRITE_SCOPE}
               AND (NOT $12::bool OR $13::uuid IS NULL OR EXISTS
                   (SELECT 1 FROM pos_projects p WHERE p.id = $13
                    AND p.workspace_id = $2 AND NOT p.archived))
             RETURNING {TASK_FIELDS}"
        );
        let row = sqlx::query(&sql)
            .bind(actor)
            .bind(workspace)
            .bind(id)
            .bind(expected_version)
            .bind(patch.title)
            .bind(has_description)
            .bind(patch.description.flatten())
            .bind(patch.priority.map(|v| v.as_str()))
            .bind(patch.status.map(|v| v.as_str()))
            .bind(has_due)
            .bind(patch.due_date.flatten())
            .bind(has_project)
            .bind(change.project_id.flatten())
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| RecordError::Unavailable)?;
        if let Some(row) = row {
            return task_from(row);
        }
        if !self.records_allowed(actor, workspace, true).await? {
            return Err(RecordError::NotFound);
        }
        if self.task_record(actor, workspace, id).await?.is_some() {
            Err(RecordError::Conflict)
        } else {
            Err(RecordError::NotFound)
        }
    }
}
