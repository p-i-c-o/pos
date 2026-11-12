//! Shared POS application operations, independent of CLI and TUI.

use pos_storage::{StorageError, TaskRepository};
use pos_tasks::{NewTask, Task, TaskPatch, ValidationError};
use std::{error::Error, fmt};

pub const PRODUCT_NAME: &str = "POS";

#[derive(Debug)]
pub enum TaskError {
    Invalid(ValidationError),
    Storage(StorageError),
    NotFound(i64),
}
impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(err) => write!(f, "{err}"),
            Self::Storage(err) => write!(f, "{err}"),
            Self::NotFound(id) => write!(f, "task #{id} not found"),
        }
    }
}
impl Error for TaskError {}
impl From<ValidationError> for TaskError {
    fn from(value: ValidationError) -> Self {
        Self::Invalid(value)
    }
}
impl From<StorageError> for TaskError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct TaskService<R: TaskRepository> {
    repository: R,
}

impl<R: TaskRepository> TaskService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn add(&self, title: &str, due_date: Option<&str>) -> Result<Task, TaskError> {
        let task = NewTask::new(title, due_date)?;
        Ok(self.repository.create(task).await?)
    }

    pub async fn list(&self, all: bool) -> Result<Vec<Task>, TaskError> {
        Ok(self.repository.list(all).await?)
    }

    pub async fn get(&self, id: i64) -> Result<Task, TaskError> {
        let task = self.repository.get(check_id(id)?).await?;
        task.ok_or(TaskError::NotFound(id))
    }

    pub async fn edit(
        &self,
        id: i64,
        title: Option<&str>,
        due_date: Option<Option<&str>>,
    ) -> Result<Task, TaskError> {
        let id = check_id(id)?;
        let patch = TaskPatch::new(title, due_date)?;
        self.repository
            .update(id, patch)
            .await?
            .ok_or(TaskError::NotFound(id))
    }

    pub async fn complete(&self, id: i64, done: bool) -> Result<Task, TaskError> {
        let id = check_id(id)?;
        self.repository
            .set_completed(id, done)
            .await?
            .ok_or(TaskError::NotFound(id))
    }

    pub async fn delete(&self, id: i64) -> Result<(), TaskError> {
        let id = check_id(id)?;
        if self.repository.delete(id).await? {
            Ok(())
        } else {
            Err(TaskError::NotFound(id))
        }
    }
}

fn check_id(id: i64) -> Result<i64, TaskError> {
    if id < 1 {
        Err(TaskError::Invalid(ValidationError(
            "task ID must be positive".into(),
        )))
    } else {
        Ok(id)
    }
}
