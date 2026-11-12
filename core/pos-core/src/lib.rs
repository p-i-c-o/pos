//! Shared POS application operations, independent of CLI and TUI.

use pos_projects::{NewProject, Project, validate_name, validate_slug};
use pos_storage::{ProjectRepository, SnapshotRepository, StorageError, TaskRepository};
use pos_tasks::{NewTask, Task, TaskPatch, ValidationError};
use std::{error::Error, fmt};

pub const PRODUCT_NAME: &str = "POS";

#[derive(Debug)]
pub enum TaskError {
    Invalid(ValidationError),
    Storage(StorageError),
    NotFound(i64),
    ProjectNotFound(String),
    ProjectArchived(String),
}
impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(err) => write!(f, "{err}"),
            Self::Storage(err) => write!(f, "{err}"),
            Self::NotFound(id) => write!(f, "task #{id} not found"),
            Self::ProjectNotFound(slug) => write!(f, "project {slug} not found"),
            Self::ProjectArchived(slug) => write!(f, "project {slug} is archived"),
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

impl<R: TaskRepository + ProjectRepository> TaskService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn add(&self, title: &str, due_date: Option<&str>) -> Result<Task, TaskError> {
        let task = NewTask::new(title, due_date)?;
        Ok(self.repository.create(task).await?)
    }

    pub async fn add_with_project(
        &self,
        title: &str,
        due_date: Option<&str>,
        project_slug: Option<&str>,
    ) -> Result<Task, TaskError> {
        let mut task = NewTask::new(title, due_date)?;
        task.project_id = match project_slug {
            Some(slug) => Some(self.resolve_project(slug, false).await?),
            None => None,
        };
        Ok(self.repository.create(task).await?)
    }

    pub async fn list_for_project(
        &self,
        all: bool,
        project_slug: Option<&str>,
    ) -> Result<Vec<Task>, TaskError> {
        match project_slug {
            None => self.list(all).await,
            Some(slug) => {
                let id = self.resolve_project(slug, true).await?;
                Ok(self.repository.list_for_project(all, id).await?)
            }
        }
    }

    pub async fn assign_project(
        &self,
        task_id: i64,
        project_slug: Option<&str>,
    ) -> Result<Task, TaskError> {
        let id = check_id(task_id)?;
        let project_id = match project_slug {
            Some(slug) => Some(self.resolve_project(slug, false).await?),
            None => None,
        };
        self.repository
            .assign_project(id, project_id)
            .await?
            .ok_or(TaskError::NotFound(id))
    }

    async fn resolve_project(&self, slug: &str, allow_archived: bool) -> Result<i64, TaskError> {
        let slug = validate_slug(slug).map_err(|err| TaskError::Invalid(ValidationError(err.0)))?;
        let project = self
            .repository
            .get_project_by_slug(&slug)
            .await?
            .ok_or_else(|| TaskError::ProjectNotFound(slug.clone()))?;
        if project.archived && !allow_archived {
            return Err(TaskError::ProjectArchived(slug));
        }
        Ok(project.id)
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

#[derive(Debug)]
pub enum ProjectError {
    Invalid(pos_projects::ValidationError),
    Storage(StorageError),
    NotFound(i64),
    SlugTaken(String),
}
impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => write!(f, "{error}"),
            Self::Storage(error) => write!(f, "{error}"),
            Self::NotFound(id) => write!(f, "project #{id} not found"),
            Self::SlugTaken(slug) => write!(f, "project slug {slug} already exists"),
        }
    }
}
impl Error for ProjectError {}
impl From<pos_projects::ValidationError> for ProjectError {
    fn from(value: pos_projects::ValidationError) -> Self {
        Self::Invalid(value)
    }
}
impl From<StorageError> for ProjectError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

pub struct ProjectService<R: ProjectRepository> {
    repository: R,
}

impl<R: ProjectRepository> ProjectService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn add(&self, name: &str, slug: Option<&str>) -> Result<Project, ProjectError> {
        let project = NewProject::new(name, slug)?;
        if self
            .repository
            .get_project_by_slug(&project.slug)
            .await?
            .is_some()
        {
            return Err(ProjectError::SlugTaken(project.slug));
        }
        Ok(self.repository.create_project(project).await?)
    }

    pub async fn list(&self, include_archived: bool) -> Result<Vec<Project>, ProjectError> {
        Ok(self.repository.list_projects(include_archived).await?)
    }

    pub async fn get(&self, id: i64) -> Result<Project, ProjectError> {
        let id = project_id(id)?;
        self.repository
            .get_project(id)
            .await?
            .ok_or(ProjectError::NotFound(id))
    }

    pub async fn rename(&self, id: i64, name: &str) -> Result<Project, ProjectError> {
        let id = project_id(id)?;
        let name = validate_name(name)?;
        self.repository
            .rename_project(id, name)
            .await?
            .ok_or(ProjectError::NotFound(id))
    }

    pub async fn archive(&self, id: i64, archived: bool) -> Result<Project, ProjectError> {
        let id = project_id(id)?;
        self.repository
            .archive_project(id, archived)
            .await?
            .ok_or(ProjectError::NotFound(id))
    }

    pub async fn delete(&self, id: i64) -> Result<(), ProjectError> {
        let id = project_id(id)?;
        if self.repository.delete_project(id).await? {
            Ok(())
        } else {
            Err(ProjectError::NotFound(id))
        }
    }
}

fn project_id(id: i64) -> Result<i64, ProjectError> {
    if id < 1 {
        Err(ProjectError::Invalid(pos_projects::ValidationError(
            "project ID must be positive".into(),
        )))
    } else {
        Ok(id)
    }
}

/// Portable JSON export format. This is a data export, not a replacement for
/// full-fidelity SQLite backups. Format changes must bump the version.
#[derive(serde::Serialize)]
pub struct LocalDataExport {
    pub format_version: u32,
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
}

/// Shared, presentation-independent export operation.
pub async fn export_local_data<R: SnapshotRepository>(
    repository: &R,
) -> Result<LocalDataExport, StorageError> {
    let snapshot = repository.snapshot().await?;
    Ok(LocalDataExport {
        format_version: 1,
        projects: snapshot.projects,
        tasks: snapshot.tasks,
    })
}
