//! Workspace membership is ALWAYS queried using the authenticated user's ID.
//! These methods are trusted-only; no workspace creation or grants are routed
//! over HTTP. Owners are implicit, and personal workspaces remain private.

use std::{error::Error, fmt};

use pos_permissions::{Action, Role, WorkspaceKind, allowed};
use sqlx::Row;
use uuid::Uuid;

use crate::PostgresStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkspaceKind,
    pub role: Role,
}

impl Workspace {
    pub fn can(&self, action: Action) -> bool {
        allowed(self.kind, Some(self.role), action)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidName,
    AlreadyExists,
    StorageUnavailable,
}
impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => f.write_str("invalid workspace name"),
            Self::AlreadyExists => f.write_str("workspace already exists"),
            Self::StorageUnavailable => f.write_str("workspace storage unavailable"),
        }
    }
}
impl Error for WorkspaceError {}

fn decode(row: sqlx::postgres::PgRow) -> Result<Workspace, WorkspaceError> {
    let kind: String = row
        .try_get("kind")
        .map_err(|_| WorkspaceError::StorageUnavailable)?;
    let role: String = row
        .try_get("role")
        .map_err(|_| WorkspaceError::StorageUnavailable)?;
    let kind = WorkspaceKind::parse(&kind).ok_or(WorkspaceError::StorageUnavailable)?;
    let role = if role == "owner" {
        Role::Owner
    } else {
        Role::membership(&role).ok_or(WorkspaceError::StorageUnavailable)?
    };
    if !allowed(kind, Some(role), Action::Read) {
        return Err(WorkspaceError::StorageUnavailable);
    }
    Ok(Workspace {
        id: row
            .try_get("id")
            .map_err(|_| WorkspaceError::StorageUnavailable)?,
        name: row
            .try_get("name")
            .map_err(|_| WorkspaceError::StorageUnavailable)?,
        kind,
        role,
    })
}

const SCOPED_WORKSPACES: &str = "\
    SELECT w.id, w.name, w.kind, \
           CASE WHEN w.owner_user_id = $1 THEN 'owner' ELSE m.role END AS role \
    FROM pos_workspaces w \
    LEFT JOIN pos_workspace_members m \
      ON m.workspace_id = w.id AND m.user_id = $1 \
    WHERE EXISTS (SELECT 1 FROM pos_users u \
                  WHERE u.id = $1 AND u.disabled_at IS NULL) \
      AND (w.owner_user_id = $1 OR (w.kind = 'shared' AND m.user_id = $1))";

impl PostgresStore {
    /// Trusted operation; fails if the owner is disabled or unknown.
    pub async fn create_workspace(
        &self,
        owner: Uuid,
        kind: WorkspaceKind,
        name: &str,
    ) -> Result<Option<Uuid>, WorkspaceError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err(WorkspaceError::InvalidName);
        }
        let result = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pos_workspaces (owner_user_id, kind, name) \
             SELECT id, $2, $3 FROM pos_users WHERE id = $1 AND disabled_at IS NULL \
             RETURNING id",
        )
        .bind(owner)
        .bind(kind.as_str())
        .bind(name)
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(id) => Ok(id),
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                Err(WorkspaceError::AlreadyExists)
            }
            Err(_) => Err(WorkspaceError::StorageUnavailable),
        }
    }

    /// Owner-only grant to an active user in a shared workspace. Never grants
    /// owner. An editor cannot invite others, including from another workspace.
    pub async fn grant_membership(
        &self,
        acting_owner: Uuid,
        workspace: Uuid,
        target_user: Uuid,
        role: Role,
    ) -> Result<bool, WorkspaceError> {
        if role == Role::Owner {
            return Ok(false);
        }
        let result = sqlx::query(
            "INSERT INTO pos_workspace_members (workspace_id, user_id, role) \
             SELECT w.id, u.id, $4 FROM pos_workspaces w \
             JOIN pos_users u ON u.id = $3 AND u.disabled_at IS NULL \
             JOIN pos_users actor ON actor.id = $2 AND actor.disabled_at IS NULL \
             WHERE w.id = $1 AND w.kind = 'shared' AND w.owner_user_id = actor.id \
               AND u.id <> w.owner_user_id \
             ON CONFLICT (workspace_id, user_id) \
             DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(workspace)
        .bind(acting_owner)
        .bind(target_user)
        .bind(role.as_str())
        .execute(&self.pool)
        .await
        .map_err(|_| WorkspaceError::StorageUnavailable)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn remove_membership(
        &self,
        acting_owner: Uuid,
        workspace: Uuid,
        target_user: Uuid,
    ) -> Result<bool, WorkspaceError> {
        let result = sqlx::query(
            "DELETE FROM pos_workspace_members m USING pos_workspaces w \
             WHERE m.workspace_id = w.id AND w.id = $1 AND w.owner_user_id = $2 \
               AND w.kind = 'shared' AND m.user_id = $3 \
               AND EXISTS (SELECT 1 FROM pos_users u WHERE u.id = $2 AND u.disabled_at IS NULL)",
        )
        .bind(workspace)
        .bind(acting_owner)
        .bind(target_user)
        .execute(&self.pool)
        .await
        .map_err(|_| WorkspaceError::StorageUnavailable)?;
        Ok(result.rows_affected() == 1)
    }

    /// Scoped list: never query the entire table and filter after fetching.
    pub async fn list_user_workspaces(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Workspace>, WorkspaceError> {
        let rows = sqlx::query(&format!("{SCOPED_WORKSPACES} ORDER BY w.created_at, w.id"))
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| WorkspaceError::StorageUnavailable)?;
        rows.into_iter().map(decode).collect()
    }

    /// A missing workspace and one belonging to another user are indistinguishable.
    pub async fn user_workspace(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<Workspace>, WorkspaceError> {
        let row = sqlx::query(&format!("{SCOPED_WORKSPACES} AND w.id = $2"))
            .bind(user_id)
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| WorkspaceError::StorageUnavailable)?;
        row.map(decode).transpose()
    }
}
