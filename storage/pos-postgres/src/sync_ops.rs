//! Atomic, device-scoped, idempotent synchronization operations.
//! The source of identity is the authenticated bearer session, never request JSON.
use pos_projects::{NewProject, validate_name};
use pos_tasks::{NewTask, TaskPatch, TaskPriority, TaskStatus};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::{error::Error, fmt};
use uuid::Uuid;

use crate::{Identity, PostgresStore, records::WRITE_SCOPE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOperationError {
    Invalid,
    NotFound,
    Conflict,
    Unavailable,
}
impl fmt::Display for SyncOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid synchronization operation",
            Self::NotFound => "workspace or record not available",
            Self::Conflict => "operation replay mismatch or stale record version",
            Self::Unavailable => "synchronization storage unavailable",
        })
    }
}
impl Error for SyncOperationError {}

/// Missing, null, and concrete values must be distinct for nullable edits.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Mutation {
    CreateProject {
        name: String,
        slug: Option<String>,
    },
    UpdateProject {
        record_id: Uuid,
        expected_version: i64,
        name: Option<String>,
        archived: Option<bool>,
    },
    CreateTask {
        title: String,
        description: Option<String>,
        due_date: Option<String>,
        project_id: Option<Uuid>,
        priority: Option<String>,
        status: Option<String>,
    },
    UpdateTask {
        record_id: Uuid,
        expected_version: i64,
        title: Option<String>,
        #[serde(default, deserialize_with = "nullable")]
        description: Option<Option<String>>,
        #[serde(default, deserialize_with = "nullable")]
        due_date: Option<Option<String>>,
        #[serde(default, deserialize_with = "nullable")]
        project_id: Option<Option<Uuid>>,
        priority: Option<String>,
        status: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncReceipt {
    pub operation_id: Uuid,
    pub record_kind: String,
    pub record_id: Uuid,
    pub revision: i64,
    pub cursor: i64,
}

fn priority(v: Option<String>) -> Result<TaskPriority, SyncOperationError> {
    v.as_deref()
        .unwrap_or("normal")
        .parse()
        .map_err(|_| SyncOperationError::Invalid)
}
fn status(v: Option<String>) -> Result<TaskStatus, SyncOperationError> {
    v.as_deref()
        .unwrap_or("todo")
        .parse()
        .map_err(|_| SyncOperationError::Invalid)
}

impl PostgresStore {
    /// Applies and records a mutation in one PostgreSQL transaction. Unique
    /// operation IDs are scoped to the *authenticated* device and workspace.
    /// A concurrent replay blocks on the unique key until the first commits,
    /// then returns exactly the same persisted receipt. Different payloads
    /// using an existing operation ID are rejected.
    pub async fn apply_sync_operation(
        &self,
        identity: Identity,
        workspace: Uuid,
        operation_id: Uuid,
        payload: Value,
    ) -> Result<SyncReceipt, SyncOperationError> {
        if operation_id.is_nil() {
            return Err(SyncOperationError::Invalid);
        }
        let mutation: Mutation =
            serde_json::from_value(payload.clone()).map_err(|_| SyncOperationError::Invalid)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SyncOperationError::Unavailable)?;
        let allowed: bool = sqlx::query_scalar(&format!(
            "SELECT ({WRITE_SCOPE}) AND EXISTS (
               SELECT 1 FROM pos_devices d WHERE d.id = $3
               AND d.user_id = $1 AND d.revoked_at IS NULL)"
        ))
        .bind(identity.user_id)
        .bind(workspace)
        .bind(identity.device_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| SyncOperationError::Unavailable)?;
        if !allowed {
            return Err(SyncOperationError::NotFound);
        }
        // Inserting a placeholder reserves the operation ID inside the same
        // transaction. Any failed validation or write rolls it back.
        let fresh = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pos_sync_operation_receipts
             (workspace_id,device_id,operation_id,request)
             VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING operation_id",
        )
        .bind(workspace)
        .bind(identity.device_id)
        .bind(operation_id)
        .bind(&payload)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SyncOperationError::Unavailable)?;
        if fresh.is_none() {
            let row = sqlx::query(
                "SELECT request,response FROM pos_sync_operation_receipts
                 WHERE workspace_id=$1 AND device_id=$2 AND operation_id=$3",
            )
            .bind(workspace)
            .bind(identity.device_id)
            .bind(operation_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| SyncOperationError::Unavailable)?
            .ok_or(SyncOperationError::Conflict)?;
            let original: Value = row
                .try_get("request")
                .map_err(|_| SyncOperationError::Unavailable)?;
            if original != payload {
                return Err(SyncOperationError::Conflict);
            }
            let response: Value = row
                .try_get("response")
                .map_err(|_| SyncOperationError::Unavailable)?;
            return serde_json::from_value(response).map_err(|_| SyncOperationError::Unavailable);
        }
        let (kind, id, revision) = match mutation {
            Mutation::CreateProject { name, slug } => {
                let p = NewProject::new(&name, slug.as_deref())
                    .map_err(|_| SyncOperationError::Invalid)?;
                let row = sqlx::query(&format!(
                    "INSERT INTO pos_projects(workspace_id,name,slug)
                     SELECT $2,$3,$4 WHERE {WRITE_SCOPE} RETURNING id,version"
                ))
                .bind(identity.user_id)
                .bind(workspace)
                .bind(p.name)
                .bind(p.slug)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    if e.as_database_error()
                        .is_some_and(|v| v.is_unique_violation())
                    {
                        SyncOperationError::Conflict
                    } else {
                        SyncOperationError::Unavailable
                    }
                })?
                .ok_or(SyncOperationError::NotFound)?;
                (
                    "project",
                    row.try_get("id")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                    row.try_get("version")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                )
            }
            Mutation::UpdateProject {
                record_id,
                expected_version,
                name,
                archived,
            } => {
                if expected_version < 1 || (name.is_none() && archived.is_none()) {
                    return Err(SyncOperationError::Invalid);
                }
                let name = name
                    .as_deref()
                    .map(validate_name)
                    .transpose()
                    .map_err(|_| SyncOperationError::Invalid)?;
                let row = sqlx::query(&format!(
                    "UPDATE pos_projects p SET name=COALESCE($5::text,p.name),
                     archived=COALESCE($6::bool,p.archived), version=p.version+1,
                     updated_at=clock_timestamp()
                     WHERE p.workspace_id=$2 AND p.id=$3 AND p.version=$4 AND {WRITE_SCOPE}
                     RETURNING p.id,p.version"
                ))
                .bind(identity.user_id)
                .bind(workspace)
                .bind(record_id)
                .bind(expected_version)
                .bind(name)
                .bind(archived)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| SyncOperationError::Unavailable)?;
                let row = row.ok_or(SyncOperationError::Conflict)?;
                (
                    "project",
                    row.try_get("id")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                    row.try_get("version")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                )
            }
            Mutation::CreateTask {
                title,
                description,
                due_date,
                project_id,
                priority: pri,
                status: state,
            } => {
                let pri = priority(pri)?;
                let state = status(state)?;
                let task = NewTask::with_details(
                    &title,
                    due_date.as_deref(),
                    description.as_deref(),
                    pri,
                    state,
                )
                .map_err(|_| SyncOperationError::Invalid)?;
                let row = sqlx::query(&format!(
                    "INSERT INTO pos_tasks(workspace_id,project_id,title,description,priority,status,due_date)
                     SELECT $2,$3::uuid,$4,$5,$6,$7,$8::date WHERE {WRITE_SCOPE}
                     AND ($3::uuid IS NULL OR EXISTS (
                       SELECT 1 FROM pos_projects p WHERE p.workspace_id=$2 AND p.id=$3 AND NOT p.archived))
                     RETURNING id,version"
                ))
                .bind(identity.user_id).bind(workspace).bind(project_id)
                .bind(task.title).bind(task.description).bind(task.priority.as_str())
                .bind(task.status.as_str()).bind(task.due_date)
                .fetch_optional(&mut *tx).await.map_err(|_| SyncOperationError::Unavailable)?
                .ok_or(SyncOperationError::NotFound)?;
                (
                    "task",
                    row.try_get("id")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                    row.try_get("version")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                )
            }
            Mutation::UpdateTask {
                record_id,
                expected_version,
                title,
                description,
                due_date,
                project_id,
                priority: pri,
                status: state,
            } => {
                if expected_version < 1 {
                    return Err(SyncOperationError::Invalid);
                }
                let pri = pri.map(|v| priority(Some(v))).transpose()?;
                let state = state.map(|v| status(Some(v))).transpose()?;
                let patch = TaskPatch::full(
                    title.as_deref(),
                    due_date.as_ref().map(|v| v.as_deref()),
                    description.as_ref().map(|v| v.as_deref()),
                    pri,
                    state,
                    project_id.map(|_| None),
                )
                .map_err(|_| SyncOperationError::Invalid)?;
                let has_desc = patch.description.is_some();
                let has_due = patch.due_date.is_some();
                let has_project = project_id.is_some();
                let row = sqlx::query(&format!(
                    "UPDATE pos_tasks t SET title=COALESCE($5::text,t.title),
                     description=CASE WHEN $6::bool THEN $7::text ELSE t.description END,
                     priority=COALESCE($8::text,t.priority), status=COALESCE($9::text,t.status),
                     due_date=CASE WHEN $10::bool THEN $11::date ELSE t.due_date END,
                     project_id=CASE WHEN $12::bool THEN $13::uuid ELSE t.project_id END,
                     version=t.version+1,updated_at=clock_timestamp()
                     WHERE t.workspace_id=$2 AND t.id=$3 AND t.version=$4 AND {WRITE_SCOPE}
                     AND (NOT $12::bool OR $13::uuid IS NULL OR EXISTS (
                       SELECT 1 FROM pos_projects p WHERE p.workspace_id=$2 AND p.id=$13 AND NOT p.archived))
                     RETURNING t.id,t.version"
                ))
                .bind(identity.user_id).bind(workspace).bind(record_id).bind(expected_version)
                .bind(patch.title).bind(has_desc).bind(patch.description.flatten())
                .bind(patch.priority.map(|v| v.as_str())).bind(patch.status.map(|v| v.as_str()))
                .bind(has_due).bind(patch.due_date.flatten()).bind(has_project).bind(project_id.flatten())
                .fetch_optional(&mut *tx).await.map_err(|_| SyncOperationError::Unavailable)?
                .ok_or(SyncOperationError::Conflict)?;
                (
                    "task",
                    row.try_get("id")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                    row.try_get("version")
                        .map_err(|_| SyncOperationError::Unavailable)?,
                )
            }
        };
        let cursor: i64 = sqlx::query_scalar(
            "SELECT cursor FROM pos_sync_workspace_clocks WHERE workspace_id=$1",
        )
        .bind(workspace)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| SyncOperationError::Unavailable)?;
        let receipt = SyncReceipt {
            operation_id,
            record_kind: kind.to_owned(),
            record_id: id,
            revision,
            cursor,
        };
        let result = serde_json::to_value(&receipt).map_err(|_| SyncOperationError::Unavailable)?;
        sqlx::query(
            "UPDATE pos_sync_operation_receipts SET response=$4
             WHERE workspace_id=$1 AND device_id=$2 AND operation_id=$3",
        )
        .bind(workspace)
        .bind(identity.device_id)
        .bind(operation_id)
        .bind(result)
        .execute(&mut *tx)
        .await
        .map_err(|_| SyncOperationError::Unavailable)?;
        tx.commit()
            .await
            .map_err(|_| SyncOperationError::Unavailable)?;
        Ok(receipt)
    }
}
