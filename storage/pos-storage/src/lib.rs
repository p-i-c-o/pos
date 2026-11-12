//! Backend-neutral task persistence interface.

use pos_projects::{NewProject, Project};
use pos_tasks::{NewTask, Task, TaskPatch};
use std::{error::Error, fmt};

#[derive(Debug)]
pub struct StorageError(pub String);
impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "storage error: {}", self.0)
    }
}
impl Error for StorageError {}

/// Async interface shared by the SQLite backend and future server-side backends.
/// All task operations are scoped to the current local database for this milestone.
// Repository futures are polled to completion on the calling thread in this milestone.
#[allow(async_fn_in_trait)]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: NewTask) -> Result<Task, StorageError>;
    async fn list(&self, include_completed: bool) -> Result<Vec<Task>, StorageError>;
    async fn list_for_project(
        &self,
        include_completed: bool,
        project_id: i64,
    ) -> Result<Vec<Task>, StorageError>;
    async fn assign_project(
        &self,
        task_id: i64,
        project_id: Option<i64>,
    ) -> Result<Option<Task>, StorageError>;
    async fn get(&self, id: i64) -> Result<Option<Task>, StorageError>;
    async fn update(&self, id: i64, patch: TaskPatch) -> Result<Option<Task>, StorageError>;
    async fn set_completed(&self, id: i64, completed: bool) -> Result<Option<Task>, StorageError>;
    async fn delete(&self, id: i64) -> Result<bool, StorageError>;
}

/// Project record storage; task relationships are maintained by the task backend.
#[allow(async_fn_in_trait)]
pub trait ProjectRepository: Send + Sync {
    async fn create_project(&self, project: NewProject) -> Result<Project, StorageError>;
    async fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>, StorageError>;
    async fn get_project(&self, id: i64) -> Result<Option<Project>, StorageError>;
    async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>, StorageError>;
    async fn rename_project(&self, id: i64, name: String) -> Result<Option<Project>, StorageError>;
    async fn archive_project(
        &self,
        id: i64,
        archived: bool,
    ) -> Result<Option<Project>, StorageError>;
    /// Deleting a project must NOT delete tasks: they become unassigned.
    async fn delete_project(&self, id: i64) -> Result<bool, StorageError>;
}

/// A database-consistent snapshot of local POS-owned records.
pub struct LocalSnapshot {
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
}

/// Read both entity tables in one database transaction so export never
/// mixes task references with projects from a different database revision.
#[allow(async_fn_in_trait)]
pub trait SnapshotRepository: Send + Sync {
    async fn snapshot(&self) -> Result<LocalSnapshot, StorageError>;
}
