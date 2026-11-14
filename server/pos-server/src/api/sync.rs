//! Read-only incremental server change feed.
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use serde::Deserialize;
use uuid::Uuid;

use super::{ApiError, ApiState, authenticate, no_store};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChangeQuery {
    #[serde(default)]
    after: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}
fn default_limit() -> i64 {
    100
}

pub(super) async fn changes(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
    Query(query): Query<ChangeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let identity = authenticate(&headers, &state).await?;
    let page = state
        .store
        .sync_changes(identity.user_id, workspace, query.after, query.limit)
        .await
        .map_err(ApiError::from)?;
    Ok((no_store(), Json(page)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationRequest {
    operation_id: Uuid,
    change: serde_json::Value,
}

pub(super) async fn apply(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
    Json(payload): Json<OperationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    use pos_postgres::SyncOperationError;
    let identity = authenticate(&headers, &state).await?;
    let receipt = state
        .store
        .apply_sync_operation(identity, workspace, payload.operation_id, payload.change)
        .await
        .map_err(|error| match error {
            SyncOperationError::Invalid => ApiError::BadRequest,
            SyncOperationError::NotFound => ApiError::NotFound,
            SyncOperationError::Conflict => ApiError::Conflict,
            SyncOperationError::Unavailable => ApiError::Unavailable,
        })?;
    Ok((no_store(), Json(receipt)))
}
