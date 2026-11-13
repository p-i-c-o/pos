//! Trusted-only user provisioning, password sign-in, device enrollment and
//! revocable sessions. No methods in this module are bound to HTTP routes.

use std::{error::Error, fmt};

use pos_auth::{
    AuthError, SessionToken, hash_password, normalize_email, token_digest, validate_device_label,
    verify_password,
};
use sqlx::Row;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::PostgresStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    InvalidInput,
    InvalidCredentials,
    AlreadyExists,
    StorageUnavailable,
}
impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidInput => "invalid identity input",
            Self::InvalidCredentials => "invalid credentials or inactive identity",
            Self::AlreadyExists => "identity already registered",
            Self::StorageUnavailable => "identity storage unavailable",
        };
        f.write_str(message)
    }
}
impl Error for IdentityError {}

impl From<AuthError> for IdentityError {
    fn from(_: AuthError) -> Self {
        Self::InvalidInput
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub user_id: Uuid,
    pub device_id: Uuid,
}

/// Returned to the trusted caller once. Never print or log the token.
pub struct Login {
    pub identity: Identity,
    pub token: SessionToken,
}

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Login")
            .field("identity", &self.identity)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl PostgresStore {
    /// Trusted bootstrap/provisioning action, NOT an anonymous registration
    /// endpoint. Caller controls authorization and account verification.
    pub async fn provision_user(&self, email: &str, password: &str) -> Result<Uuid, IdentityError> {
        let email = normalize_email(email)?;
        let password = Zeroizing::new(password.to_owned());
        let hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?
            .map_err(|_| IdentityError::InvalidInput)?;

        let result = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pos_users (login_email, password_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(email)
        .bind(hash)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(id) => Ok(id),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(IdentityError::AlreadyExists)
            }
            Err(_) => Err(IdentityError::StorageUnavailable),
        }
    }

    /// Trusted-only atomic account bootstrap: create a user and exactly one
    /// owner-only personal workspace in a single transaction.
    /// Unlike two separate calls, failure cannot strand an account without
    /// its initial workspace.
    pub async fn provision_user_with_personal_workspace(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(Uuid, Uuid), IdentityError> {
        let email = normalize_email(email)?;
        let password = Zeroizing::new(password.to_owned());
        let hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?
            .map_err(|_| IdentityError::InvalidInput)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?;
        let user = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pos_users (login_email, password_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(email)
        .bind(hash)
        .fetch_one(&mut *tx)
        .await;
        let user = match user {
            Ok(id) => id,
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                return Err(IdentityError::AlreadyExists);
            }
            Err(_) => return Err(IdentityError::StorageUnavailable),
        };
        let workspace = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pos_workspaces (owner_user_id, kind, name) \\
             VALUES ($1, 'personal', 'Personal') RETURNING id",
        )
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?;
        Ok((user, workspace))
    }

    /// Password authentication optionally reuses an active device ID.
    /// A new device is enrolled only after successful password verification.
    /// Sessions are strictly tied to one active user and device, and expire
    /// server-side after 24 hours. No token plaintext is stored in PostgreSQL.
    pub async fn sign_in(
        &self,
        email: &str,
        password: &str,
        device_label: &str,
        existing_device: Option<Uuid>,
    ) -> Result<Login, IdentityError> {
        let email = normalize_email(email).map_err(|_| IdentityError::InvalidCredentials)?;
        let device_label = validate_device_label(device_label)?;
        let row = sqlx::query(
            "SELECT id, password_hash FROM pos_users \
             WHERE login_email = $1 AND disabled_at IS NULL",
        )
        .bind(&email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;

        let user = row.as_ref().and_then(|r| r.try_get::<Uuid, _>("id").ok());
        let hash = row.and_then(|r| r.try_get::<String, _>("password_hash").ok());

        // Perform Argon2 computation outside Tokio worker threads. Unknown
        // identities still pay a comparable hash cost, reducing enumeration.
        let password = Zeroizing::new(password.to_owned());
        let authenticated = tokio::task::spawn_blocking(move || match hash {
            Some(stored) => verify_password(&password, &stored),
            None => {
                let _ = hash_password("unknown-account-placeholder-credential");
                false
            }
        })
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        if !authenticated {
            return Err(IdentityError::InvalidCredentials);
        }
        let user = user.ok_or(IdentityError::InvalidCredentials)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?;
        // Serialize disabling a user with new logins.
        let active = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM pos_users WHERE id = $1 \
             AND disabled_at IS NULL FOR UPDATE",
        )
        .bind(user)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        if active.is_none() {
            return Err(IdentityError::InvalidCredentials);
        }

        let device = if let Some(id) = existing_device {
            let valid = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM pos_devices \
                 WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL FOR UPDATE",
            )
            .bind(id)
            .bind(user)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?;
            valid.ok_or(IdentityError::InvalidCredentials)?
        } else {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO pos_devices (user_id, label) \
                 VALUES ($1, $2) RETURNING id",
            )
            .bind(user)
            .bind(device_label)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?
        };

        let token = SessionToken::generate();
        let digest = token.digest();
        sqlx::query(
            "INSERT INTO pos_sessions (token_hash, user_id, device_id, expires_at) \
             VALUES ($1, $2, $3, CURRENT_TIMESTAMP + INTERVAL '24 hours')",
        )
        .bind(digest.as_slice())
        .bind(user)
        .bind(device)
        .execute(&mut *tx)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| IdentityError::StorageUnavailable)?;

        Ok(Login {
            identity: Identity {
                user_id: user,
                device_id: device,
            },
            token,
        })
    }

    /// Fail closed on expiry, session revocation, device revocation and
    /// user disablement. The caller must additionally authorize scope.
    pub async fn authenticate_token(
        &self,
        bearer: &str,
    ) -> Result<Option<Identity>, IdentityError> {
        let Some(digest) = token_digest(bearer) else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT s.user_id, s.device_id \
             FROM pos_sessions s \
             JOIN pos_users u ON u.id = s.user_id \
             JOIN pos_devices d ON d.id = s.device_id AND d.user_id = s.user_id \
             WHERE s.token_hash = $1 AND s.expires_at > CURRENT_TIMESTAMP \
             AND s.revoked_at IS NULL AND u.disabled_at IS NULL AND d.revoked_at IS NULL",
        )
        .bind(digest.as_slice())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        row.map(|row| {
            Ok(Identity {
                user_id: row
                    .try_get("user_id")
                    .map_err(|_| IdentityError::StorageUnavailable)?,
                device_id: row
                    .try_get("device_id")
                    .map_err(|_| IdentityError::StorageUnavailable)?,
            })
        })
        .transpose()
    }

    /// Anyone possessing the token can revoke that token; does not permit
    /// revocation of other sessions without trusted/authorized actions.
    pub async fn revoke_token(&self, bearer: &str) -> Result<bool, IdentityError> {
        let Some(digest) = token_digest(bearer) else {
            return Ok(false);
        };
        let affected = sqlx::query(
            "UPDATE pos_sessions SET revoked_at = CURRENT_TIMESTAMP \
             WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(digest.as_slice())
        .execute(&self.pool)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        Ok(affected.rows_affected() == 1)
    }

    /// Trusted-only device-revocation action, explicitly scoped to user ID.
    pub async fn revoke_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<bool, IdentityError> {
        let affected = sqlx::query(
            "UPDATE pos_devices SET revoked_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(device_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        Ok(affected.rows_affected() == 1)
    }

    /// Trusted-only account-disable action, revokes access immediately.
    pub async fn disable_user(&self, user_id: Uuid) -> Result<bool, IdentityError> {
        let affected = sqlx::query(
            "UPDATE pos_users SET disabled_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND disabled_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| IdentityError::StorageUnavailable)?;
        Ok(affected.rows_affected() == 1)
    }
}
