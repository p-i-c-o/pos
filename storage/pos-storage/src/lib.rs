//! Backend-neutral task persistence interface.

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
    async fn get(&self, id: i64) -> Result<Option<Task>, StorageError>;
    async fn update(&self, id: i64, patch: TaskPatch) -> Result<Option<Task>, StorageError>;
    async fn set_completed(&self, id: i64, completed: bool) -> Result<Option<Task>, StorageError>;
    async fn delete(&self, id: i64) -> Result<bool, StorageError>;
}
