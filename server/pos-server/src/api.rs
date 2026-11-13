//! Authenticated, read-only API surface.
//! No login, registration, workspace administration or Tasks/Projects routes.
//! Authorization is evaluated afresh for each request against PostgreSQL.

mod records;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pos_permissions::{Action, allowed};
use pos_postgres::{Identity, PostgresStore, Workspace};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone)]
struct ApiState {
    store: PostgresStore,
}

#[derive(Serialize)]
struct ApiErrorMessage {
    code: &'static str,
    message: &'static str,
}
#[derive(Serialize)]
struct ApiErrorResponse {
    error: ApiErrorMessage,
}

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    BadRequest,
    Conflict,
    NotFound,
    Unavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "invalid or missing bearer token",
            ),
            Self::BadRequest => (StatusCode::BAD_REQUEST, "bad_request", "invalid request"),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "conflict",
                "stale version or duplicate record",
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource not found"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "authentication service unavailable",
            ),
        };
        (
            status,
            no_store(),
            Json(ApiErrorResponse {
                error: ApiErrorMessage { code, message },
            }),
        )
            .into_response()
    }
}

fn no_store() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers
}

async fn authenticate(headers: &HeaderMap, state: &ApiState) -> Result<Identity, ApiError> {
    // Reject duplicated Authorization headers rather than guessing which
    // value a reverse proxy or server would select.
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next().ok_or(ApiError::Unauthorized)?;
    if values.next().is_some() {
        return Err(ApiError::Unauthorized);
    }
    let value = value.to_str().map_err(|_| ApiError::Unauthorized)?;
    let bearer = value
        .strip_prefix("Bearer ")
        .ok_or(ApiError::Unauthorized)?;

    state
        .store
        .authenticate_token(bearer)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::Unauthorized)
}

#[derive(Serialize)]
struct MeResponse {
    user_id: Uuid,
    device_id: Uuid,
}
async fn me(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let identity = authenticate(&headers, &state).await?;
    Ok((
        no_store(),
        Json(MeResponse {
            user_id: identity.user_id,
            device_id: identity.device_id,
        }),
    ))
}

#[derive(Serialize)]
struct WorkspaceResponse {
    id: Uuid,
    name: String,
    kind: &'static str,
    role: &'static str,
    can_read: bool,
    can_write: bool,
    can_manage_members: bool,
}

fn view(workspace: Workspace) -> WorkspaceResponse {
    WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        kind: workspace.kind.as_str(),
        role: workspace.role.as_str(),
        can_read: allowed(workspace.kind, Some(workspace.role), Action::Read),
        can_write: allowed(workspace.kind, Some(workspace.role), Action::Write),
        can_manage_members: allowed(workspace.kind, Some(workspace.role), Action::ManageMembers),
    }
}

async fn workspaces(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let identity = authenticate(&headers, &state).await?;
    let records = state
        .store
        .list_user_workspaces(identity.user_id)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    Ok((
        no_store(),
        Json(records.into_iter().map(view).collect::<Vec<_>>()),
    ))
}

async fn workspace(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let identity = authenticate(&headers, &state).await?;
    let record = state
        .store
        .user_workspace(identity.user_id, id)
        .await
        .map_err(|_| ApiError::Unavailable)?
        .ok_or(ApiError::NotFound)?;
    Ok((no_store(), Json(view(record))))
}

pub(crate) fn routes(store: PostgresStore) -> Router {
    Router::new()
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/workspaces", get(workspaces))
        .route("/api/v1/workspaces/{id}", get(workspace))
        .route(
            "/api/v1/workspaces/{id}/projects",
            get(records::list_projects).post(records::create_project),
        )
        .route(
            "/api/v1/workspaces/{id}/projects/{project}",
            get(records::get_project).patch(records::update_project),
        )
        .route(
            "/api/v1/workspaces/{id}/tasks",
            get(records::list_tasks).post(records::create_task),
        )
        .route(
            "/api/v1/workspaces/{id}/tasks/{task}",
            get(records::get_task).patch(records::update_task),
        )
        .with_state(ApiState { store })
}
