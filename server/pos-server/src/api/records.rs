//! Versioned HTTP transport for workspace-scoped server records.
//! Domain validation and authorization are performed by pos-postgres;
//! transport payloads never contain trusted user IDs.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use pos_postgres::{CreateTaskRecord, RecordError, TaskChanges};
use pos_tasks::{TaskPriority, TaskStatus};
use serde::{Deserialize, Deserializer};
use uuid::Uuid;

use super::{ApiError, ApiState, authenticate, no_store};

impl From<RecordError> for ApiError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Invalid => Self::BadRequest,
            RecordError::NotFound => Self::NotFound,
            RecordError::Conflict => Self::Conflict,
            RecordError::Unavailable => Self::Unavailable,
        }
    }
}

/// Capture three states for nullable PATCH fields: not provided, explicit
/// null (clear), and a concrete value.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewProjectRequest {
    name: String,
    slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateProjectRequest {
    expected_version: i64,
    name: Option<String>,
    archived: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewTaskRequest {
    title: String,
    description: Option<String>,
    due_date: Option<String>,
    project_id: Option<Uuid>,
    priority: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateTaskRequest {
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
}

fn priority(raw: Option<&str>) -> Result<TaskPriority, ApiError> {
    raw.unwrap_or("normal")
        .parse()
        .map_err(|_| ApiError::BadRequest)
}
fn status(raw: Option<&str>) -> Result<TaskStatus, ApiError> {
    raw.unwrap_or("todo")
        .parse()
        .map_err(|_| ApiError::BadRequest)
}

pub(super) async fn create_project(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
    Json(payload): Json<NewProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let record = state
        .store
        .create_project_record(user, workspace, &payload.name, payload.slug.as_deref())
        .await?;
    Ok((StatusCode::CREATED, no_store(), Json(record)))
}

pub(super) async fn list_projects(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    Ok((
        no_store(),
        Json(state.store.list_project_records(user, workspace).await?),
    ))
}

pub(super) async fn get_project(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path((workspace, project)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let record = state
        .store
        .project_record(user, workspace, project)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((no_store(), Json(record)))
}

pub(super) async fn update_project(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path((workspace, project)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let record = state
        .store
        .update_project_record(
            user,
            workspace,
            project,
            payload.expected_version,
            payload.name.as_deref(),
            payload.archived,
        )
        .await?;
    Ok((no_store(), Json(record)))
}

pub(super) async fn create_task(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
    Json(payload): Json<NewTaskRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let record = state
        .store
        .create_task_record(
            user,
            workspace,
            &CreateTaskRecord {
                title: payload.title,
                description: payload.description,
                priority: priority(payload.priority.as_deref())?,
                status: status(payload.status.as_deref())?,
                due_date: payload.due_date,
                project_id: payload.project_id,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, no_store(), Json(record)))
}

pub(super) async fn list_tasks(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(workspace): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    Ok((
        no_store(),
        Json(state.store.list_task_records(user, workspace).await?),
    ))
}

pub(super) async fn get_task(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path((workspace, task)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let record = state
        .store
        .task_record(user, workspace, task)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((no_store(), Json(record)))
}

pub(super) async fn update_task(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path((workspace, task)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = authenticate(&headers, &state).await?.user_id;
    let changes = TaskChanges {
        title: payload.title,
        description: payload.description,
        due_date: payload.due_date,
        project_id: payload.project_id,
        priority: payload
            .priority
            .as_deref()
            .map(|v| v.parse().map_err(|_| ApiError::BadRequest))
            .transpose()?,
        status: payload
            .status
            .as_deref()
            .map(|v| v.parse().map_err(|_| ApiError::BadRequest))
            .transpose()?,
    };
    let record = state
        .store
        .update_task_record(user, workspace, task, payload.expected_version, &changes)
        .await?;
    Ok((no_store(), Json(record)))
}
