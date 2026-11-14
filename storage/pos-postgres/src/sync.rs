//! Durable, commit-ordered, workspace-scoped changes for incremental downloads.
//! No upload API exists yet; server CRUD writes are captured automatically.
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{PostgresStore, RecordError};

#[derive(Debug, Serialize)]
pub struct SyncChange {
    pub cursor: i64,
    pub record_kind: String,
    pub record_id: Uuid,
    pub revision: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SyncPage {
    pub changes: Vec<SyncChange>,
    pub next_cursor: i64,
    pub has_more: bool,
}

/// Authorization is part of the SQL query, not just HTTP middleware.
impl PostgresStore {
    pub async fn sync_changes(
        &self,
        actor: Uuid,
        workspace: Uuid,
        after: i64,
        limit: i64,
    ) -> Result<SyncPage, RecordError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(RecordError::Invalid);
        }
        if !self.records_allowed(actor, workspace, false).await? {
            return Err(RecordError::NotFound);
        }
        let rows = sqlx::query(
            "SELECT c.cursor, c.record_kind, c.record_id, c.revision, c.data
             FROM pos_sync_changes c WHERE c.workspace_id = $2 AND c.cursor > $3
               AND EXISTS (
                 SELECT 1 FROM pos_workspaces w
                 JOIN pos_users actor ON actor.id = $1 AND actor.disabled_at IS NULL
                 LEFT JOIN pos_workspace_members m ON m.workspace_id = w.id AND m.user_id = $1
                 WHERE w.id = c.workspace_id AND
                   (w.owner_user_id = $1 OR (w.kind = 'shared' AND m.role IN ('viewer','editor')))
               )
             ORDER BY c.cursor ASC LIMIT $4",
        )
        .bind(actor)
        .bind(workspace)
        .bind(after)
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| RecordError::Unavailable)?;
        let has_more = rows.len() as i64 > limit;
        let mut changes = Vec::new();
        for row in rows.into_iter().take(limit as usize) {
            changes.push(SyncChange {
                cursor: row
                    .try_get("cursor")
                    .map_err(|_| RecordError::Unavailable)?,
                record_kind: row
                    .try_get("record_kind")
                    .map_err(|_| RecordError::Unavailable)?,
                record_id: row
                    .try_get("record_id")
                    .map_err(|_| RecordError::Unavailable)?,
                revision: row
                    .try_get("revision")
                    .map_err(|_| RecordError::Unavailable)?,
                data: row.try_get("data").map_err(|_| RecordError::Unavailable)?,
            });
        }
        let next_cursor = changes.last().map_or(after, |change| change.cursor);
        Ok(SyncPage {
            changes,
            next_cursor,
            has_more,
        })
    }
}
