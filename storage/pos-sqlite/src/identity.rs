//! Persistent local identity and one immutable server-workspace binding.
//! Mapping writes are internal-only foundations, not a sync transport.
use crate::{SqliteTaskRepository, storage_err};
use pos_entities::LocalRecordId;
use pos_storage::StorageError;
use pos_sync::RecordKind;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncBinding {
    pub database_id: Uuid,
    pub server_origin: String,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub workspace_id: Uuid,
    pub canonical_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncState {
    pub database_id: Uuid,
    pub binding: Option<SyncBinding>,
    pub journal_entries: i64,
    pub mapped_records: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordMapping {
    pub kind: RecordKind,
    pub local_id: LocalRecordId,
    pub server_id: Uuid,
    pub server_version: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindOutcome {
    Created,
    AlreadyBound,
}

fn kind_name(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Task => "task",
        RecordKind::Project => "project",
    }
}

fn parse_uuid(value: String) -> Result<Uuid, StorageError> {
    Uuid::parse_str(&value).map_err(storage_err)
}

fn decode_binding(row: &sqlx::sqlite::SqliteRow) -> Result<SyncBinding, StorageError> {
    Ok(SyncBinding {
        database_id: parse_uuid(row.try_get("database_id").map_err(storage_err)?)?,
        server_origin: row.try_get("server_origin").map_err(storage_err)?,
        user_id: parse_uuid(row.try_get("user_id").map_err(storage_err)?)?,
        device_id: parse_uuid(row.try_get("device_id").map_err(storage_err)?)?,
        workspace_id: parse_uuid(row.try_get("workspace_id").map_err(storage_err)?)?,
        canonical_path: row.try_get("canonical_path").map_err(storage_err)?,
    })
}

impl SqliteTaskRepository {
    /// Snapshot metadata without changing rows, migrations or journal state.
    pub async fn sync_state(&self) -> Result<SyncState, StorageError> {
        let mut tx = self.pool.begin().await.map_err(storage_err)?;
        let id: String =
            sqlx::query_scalar("SELECT database_id FROM pos_sync_database WHERE slot = 1")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage_err)?;
        let binding = sqlx::query(
            "SELECT database_id,server_origin,user_id,device_id,workspace_id,canonical_path
             FROM pos_sync_binding WHERE slot = 1",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_err)?
        .as_ref()
        .map(decode_binding)
        .transpose()?;
        let journal_entries: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_sync_outbox")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_err)?;
        let mapped_records: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_sync_record_map")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_err)?;
        tx.commit().await.map_err(storage_err)?;
        let database_id = parse_uuid(id)?;
        if binding
            .as_ref()
            .is_some_and(|b| b.database_id != database_id)
        {
            return Err(StorageError("database/binding identity mismatch".into()));
        }
        Ok(SyncState {
            database_id,
            binding,
            journal_entries,
            mapped_records,
        })
    }

    /// Caller must separately authenticate server and reserve the path anchor.
    /// No rebinding to another workspace, account, device or database path.
    pub async fn bind_workspace_once(
        &self,
        requested: &SyncBinding,
    ) -> Result<BindOutcome, StorageError> {
        let mut tx = self.pool.begin().await.map_err(storage_err)?;
        let actual: String =
            sqlx::query_scalar("SELECT database_id FROM pos_sync_database WHERE slot = 1")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage_err)?;
        if parse_uuid(actual)? != requested.database_id {
            return Err(StorageError("wrong local database identity".into()));
        }
        let existing = sqlx::query(
            "SELECT database_id,server_origin,user_id,device_id,workspace_id,canonical_path
             FROM pos_sync_binding WHERE slot = 1",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_err)?;
        if let Some(row) = existing {
            return if &decode_binding(&row)? == requested {
                Ok(BindOutcome::AlreadyBound)
            } else {
                Err(StorageError(
                    "database already bound; refusing workspace/device switch".into(),
                ))
            };
        }
        if requested.server_origin.len() > 255 || requested.canonical_path.is_empty() {
            return Err(StorageError("invalid workspace binding data".into()));
        }
        sqlx::query(
            "INSERT INTO pos_sync_binding
             (slot,database_id,server_origin,user_id,device_id,workspace_id,canonical_path)
             VALUES (1,?,?,?,?,?,?)",
        )
        .bind(requested.database_id.to_string())
        .bind(&requested.server_origin)
        .bind(requested.user_id.to_string())
        .bind(requested.device_id.to_string())
        .bind(requested.workspace_id.to_string())
        .bind(&requested.canonical_path)
        .execute(&mut *tx)
        .await
        .map_err(storage_err)?;
        tx.commit().await.map_err(storage_err)?;
        Ok(BindOutcome::Created)
    }

    /// Internal mapping primitive: ONLY call after verified server response.
    /// There is no CLI action or sync worker invoking it in PR #19.
    pub async fn record_mapping_once(&self, mapping: &RecordMapping) -> Result<(), StorageError> {
        if mapping.server_version < 1 || mapping.server_id.is_nil() {
            return Err(StorageError("invalid server mapping revision/ID".into()));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pos_sync_outbox
             WHERE record_kind = ? AND sync_id = ?)",
        )
        .bind(kind_name(mapping.kind))
        .bind(mapping.local_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_err)?;
        if exists != 1 {
            return Err(StorageError("unknown local record".into()));
        }
        sqlx::query(
            "INSERT INTO pos_sync_record_map
             (record_kind,local_sync_id,server_record_id,server_version)
             VALUES (?,?,?,?)",
        )
        .bind(kind_name(mapping.kind))
        .bind(mapping.local_id.to_string())
        .bind(mapping.server_id.to_string())
        .bind(mapping.server_version)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?;
        Ok(())
    }

    pub async fn record_mapping(
        &self,
        kind: RecordKind,
        local_id: LocalRecordId,
    ) -> Result<Option<RecordMapping>, StorageError> {
        let row = sqlx::query(
            "SELECT server_record_id,server_version FROM pos_sync_record_map
             WHERE record_kind = ? AND local_sync_id = ?",
        )
        .bind(kind_name(kind))
        .bind(local_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_err)?;
        row.map(|row| {
            Ok(RecordMapping {
                kind,
                local_id,
                server_id: parse_uuid(row.try_get("server_record_id").map_err(storage_err)?)?,
                server_version: row.try_get("server_version").map_err(storage_err)?,
            })
        })
        .transpose()
    }

    /// CAS update after separately verified server acknowledgement only.
    pub async fn advance_mapping_version(
        &self,
        kind: RecordKind,
        local_id: LocalRecordId,
        expected: i64,
        confirmed: i64,
    ) -> Result<(), StorageError> {
        if expected < 1 || confirmed <= expected {
            return Err(StorageError("server version must increase".into()));
        }
        let count = sqlx::query(
            "UPDATE pos_sync_record_map SET server_version = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE record_kind = ? AND local_sync_id = ? AND server_version = ?",
        )
        .bind(confirmed)
        .bind(kind_name(kind))
        .bind(local_id.to_string())
        .bind(expected)
        .execute(&self.pool)
        .await
        .map_err(storage_err)?
        .rows_affected();
        if count != 1 {
            return Err(StorageError(
                "stale mapping revision; preserve conflict".into(),
            ));
        }
        Ok(())
    }
}
