//! Read-only inspection of the durable, append-only local sync journal.
//!
//! Deliberately no acknowledgement, upload, dequeue or conflict-resolution
//! method exists. SQL triggers atomically record all local mutations.

use pos_entities::LocalRecordId;
use pos_storage::StorageError;
use pos_sync::{ChangeOperation, PendingChange, RecordKind};
use sqlx::Row;

use crate::{SqliteTaskRepository, storage_err};

impl SqliteTaskRepository {
    /// Immutable change snapshots in local sequence order.
    ///
    /// An after_seq cursor is exclusive. Changes are never automatically
    /// removed; a future sync protocol needs explicit, verified acknowledgments.
    pub async fn pending_sync_changes(
        &self,
        after_seq: i64,
        limit: u32,
    ) -> Result<Vec<PendingChange>, StorageError> {
        if after_seq < 0 || !(1..=1000).contains(&limit) {
            return Err(StorageError("invalid journal cursor or page size".into()));
        }
        let rows = sqlx::query(
            "SELECT seq, format_version, record_kind, sync_id, operation, payload_json, created_at
             FROM pos_sync_outbox WHERE seq > ? ORDER BY seq LIMIT ?",
        )
        .bind(after_seq)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_err)?;

        rows.into_iter()
            .map(|row| {
                let kind = match row
                    .try_get::<String, _>("record_kind")
                    .map_err(storage_err)?
                    .as_str()
                {
                    "task" => RecordKind::Task,
                    "project" => RecordKind::Project,
                    _ => return Err(StorageError("unrecognized sync record kind".into())),
                };
                let operation = match row
                    .try_get::<String, _>("operation")
                    .map_err(storage_err)?
                    .as_str()
                {
                    "upsert" => ChangeOperation::Upsert,
                    "delete" => ChangeOperation::Delete,
                    _ => return Err(StorageError("unrecognized sync operation".into())),
                };
                let uid: String = row.try_get("sync_id").map_err(storage_err)?;
                let payload: Option<String> = row.try_get("payload_json").map_err(storage_err)?;
                let change = PendingChange {
                    seq: row.try_get("seq").map_err(storage_err)?,
                    format_version: row.try_get("format_version").map_err(storage_err)?,
                    kind,
                    sync_id: LocalRecordId::parse(&uid).map_err(storage_err)?,
                    operation,
                    payload: payload
                        .map(|text| serde_json::from_str(&text).map_err(storage_err))
                        .transpose()?,
                    created_at: row.try_get("created_at").map_err(storage_err)?,
                };
                if !change.is_valid() {
                    return Err(StorageError("invalid queued sync change".into()));
                }
                Ok(change)
            })
            .collect()
    }

    /// The UUID is stable across local edits, renames, backup/restore and
    /// migration reruns. It is not yet a server UUID or a server mapping.
    pub async fn task_sync_id(&self, id: i64) -> Result<Option<LocalRecordId>, StorageError> {
        lookup_id(&self.pool, "SELECT sync_id FROM tasks WHERE id = ?", id).await
    }

    pub async fn project_sync_id(&self, id: i64) -> Result<Option<LocalRecordId>, StorageError> {
        lookup_id(&self.pool, "SELECT sync_id FROM projects WHERE id = ?", id).await
    }
}

async fn lookup_id(
    pool: &sqlx::SqlitePool,
    sql: &str,
    id: i64,
) -> Result<Option<LocalRecordId>, StorageError> {
    let record: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(storage_err)?;
    record
        .map(|id| LocalRecordId::parse(&id).map_err(storage_err))
        .transpose()
}
