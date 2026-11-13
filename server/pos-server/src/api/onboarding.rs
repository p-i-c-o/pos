//! Development-only, loopback HTTP password login and self-service logout.
//! No account creation through HTTP, no cookie sessions and no public exposure.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use pos_postgres::IdentityError;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{ApiError, ApiState, authenticate, no_store};

/// A conservative server-wide limiter. All failed and successful attempts
/// count, so neither unknown accounts nor bad passwords bypass the budget.
/// This is only appropriate for the development loopback server, not an
/// Internet-facing login service.
#[derive(Default)]
pub(super) struct LoginLimit {
    recent: Mutex<VecDeque<Instant>>,
}

impl LoginLimit {
    async fn check(&self) -> Result<(), ApiError> {
        const MAX_ATTEMPTS: usize = 8;
        const WINDOW: Duration = Duration::from_secs(60);
        let now = Instant::now();
        let mut attempts = self.recent.lock().await;
        while attempts
            .front()
            .is_some_and(|time| now.duration_since(*time) >= WINDOW)
        {
            attempts.pop_front();
        }
        if attempts.len() >= MAX_ATTEMPTS {
            return Err(ApiError::RateLimited);
        }
        attempts.push_back(now);
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoginRequest {
    email: String,
    password: String,
    device_label: String,
    existing_device: Option<Uuid>,
}

#[derive(Serialize)]
struct LoginResponse {
    user_id: Uuid,
    device_id: Uuid,
    token: String,
    expires_in_seconds: u64,
}

pub(super) async fn login(
    State(state): State<ApiState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    state.login_limit.check().await?;

    let password = Zeroizing::new(request.password);
    let session = state
        .store
        .sign_in(
            &request.email,
            &password,
            &request.device_label,
            request.existing_device,
        )
        .await
        .map_err(|error| match error {
            // Do not distinguish unknown accounts, revoked devices or bad
            // credentials. Device label validation cannot reveal an account.
            IdentityError::InvalidCredentials => ApiError::Unauthorized,
            IdentityError::InvalidInput => ApiError::BadRequest,
            IdentityError::AlreadyExists => ApiError::Conflict,
            IdentityError::StorageUnavailable => ApiError::Unavailable,
        })?;

    let body = LoginResponse {
        user_id: session.identity.user_id,
        device_id: session.identity.device_id,
        token: session.token.expose().to_owned(),
        expires_in_seconds: 24 * 3600,
    };
    Ok((no_store(), Json(body)))
}

/// Revokes only the presented credential. Other devices remain active.
pub(super) async fn logout(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let mut values = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    let one = values.next().ok_or(ApiError::Unauthorized)?;
    if values.next().is_some() {
        return Err(ApiError::Unauthorized);
    }
    let bearer = one
        .to_str()
        .ok()
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;

    // Validate that the token is currently active before revoking it.
    authenticate(&headers, &state).await?;
    state
        .store
        .revoke_token(bearer)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    Ok((no_store(), StatusCode::NO_CONTENT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_budget_counts_attempts_and_limits_bursts() {
        let limit = LoginLimit::default();
        for _ in 0..8 {
            assert!(limit.check().await.is_ok());
        }
        assert!(matches!(limit.check().await, Err(ApiError::RateLimited)));
    }
}
