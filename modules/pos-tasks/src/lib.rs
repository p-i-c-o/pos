//! Task domain model, filters and validation. No storage or UI code lives here.

use chrono::NaiveDate;
use serde::Serialize;
use std::{error::Error, fmt, str::FromStr};

pub const MAX_TITLE_LEN: usize = 500;
pub const MAX_DESCRIPTION_LEN: usize = 20_000;
pub const MAX_SEARCH_LEN: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl TaskPriority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}
impl fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for TaskPriority {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            _ => Err(ValidationError(
                "priority must be low, normal, high or urgent".into(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    Blocked,
    Completed,
}
impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in-progress",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
        }
    }
}
impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for TaskStatus {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "todo" => Ok(Self::Todo),
            "in-progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "completed" => Ok(Self::Completed),
            _ => Err(ValidationError(
                "status must be todo, in-progress, blocked or completed".into(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskSort {
    #[default]
    Due,
    Priority,
    Created,
}
impl FromStr for TaskSort {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "due" => Ok(Self::Due),
            "priority" => Ok(Self::Priority),
            "created" => Ok(Self::Created),
            _ => Err(ValidationError(
                "sort must be due, priority or created".into(),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub due_date: Option<String>,
    pub project_id: Option<i64>,
    /// Kept for compatibility with existing CLI, TUI and JSON clients.
    /// Always equivalent to status == completed.
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct NewTask {
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub due_date: Option<String>,
    pub project_id: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct TaskPatch {
    pub title: Option<String>,
    /// None = unchanged; Some(None) = remove due date.
    pub due_date: Option<Option<String>>,
    /// None = unchanged; Some(None) = clear description.
    pub description: Option<Option<String>>,
    pub priority: Option<TaskPriority>,
    pub status: Option<TaskStatus>,
    /// None = unchanged; Some(None) = remove project assignment.
    pub project_id: Option<Option<i64>>,
}

#[derive(Clone, Debug, Default)]
pub struct TaskQuery {
    pub include_completed: bool,
    pub project_id: Option<i64>,
    pub search: Option<String>,
    pub priority: Option<TaskPriority>,
    pub status: Option<TaskStatus>,
    pub due_before: Option<String>,
    pub sort: TaskSort,
}

impl TaskQuery {
    pub fn new(
        include_completed: bool,
        project_id: Option<i64>,
        search: Option<&str>,
        priority: Option<TaskPriority>,
        status: Option<TaskStatus>,
        due_before: Option<&str>,
        sort: TaskSort,
    ) -> Result<Self, ValidationError> {
        let search = search.map(valid_search).transpose()?;
        let due_before = due_before.map(valid_due_date).transpose()?;
        Ok(Self {
            include_completed,
            project_id,
            search,
            priority,
            status,
            due_before,
            sort,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Error for ValidationError {}

fn valid_title(title: &str) -> Result<String, ValidationError> {
    let title = title.trim();
    if title.is_empty() || title.len() > MAX_TITLE_LEN || title.contains('\0') {
        return Err(ValidationError(format!(
            "task title must be 1 to {MAX_TITLE_LEN} bytes and contain no NUL"
        )));
    }
    Ok(title.to_string())
}

fn valid_due_date(due: &str) -> Result<String, ValidationError> {
    if due.len() != 10
        || !due.as_bytes().iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
        || NaiveDate::parse_from_str(due, "%Y-%m-%d").is_err()
    {
        return Err(ValidationError(
            "due date must be a real calendar date in YYYY-MM-DD format".into(),
        ));
    }
    Ok(due.to_string())
}

fn valid_description(description: &str) -> Result<String, ValidationError> {
    if description.len() > MAX_DESCRIPTION_LEN || description.contains('\0') {
        return Err(ValidationError(format!(
            "description must be at most {MAX_DESCRIPTION_LEN} bytes and contain no NUL"
        )));
    }
    Ok(description.to_string())
}

fn valid_search(search: &str) -> Result<String, ValidationError> {
    let search = search.trim();
    if search.is_empty() || search.len() > MAX_SEARCH_LEN || search.contains('\0') {
        return Err(ValidationError(format!(
            "search must be 1 to {MAX_SEARCH_LEN} bytes and contain no NUL"
        )));
    }
    Ok(search.to_string())
}

impl NewTask {
    pub fn new(title: &str, due_date: Option<&str>) -> Result<Self, ValidationError> {
        Ok(Self {
            title: valid_title(title)?,
            due_date: due_date.map(valid_due_date).transpose()?,
            project_id: None,
            description: None,
            priority: TaskPriority::Normal,
            status: TaskStatus::Todo,
        })
    }

    pub fn with_details(
        title: &str,
        due_date: Option<&str>,
        description: Option<&str>,
        priority: TaskPriority,
        status: TaskStatus,
    ) -> Result<Self, ValidationError> {
        let mut task = Self::new(title, due_date)?;
        task.description = description.map(valid_description).transpose()?;
        task.priority = priority;
        task.status = status;
        Ok(task)
    }
}

impl TaskPatch {
    pub fn new(
        title: Option<&str>,
        due_date: Option<Option<&str>>,
    ) -> Result<Self, ValidationError> {
        Self::full(title, due_date, None, None, None, None)
    }

    pub fn full(
        title: Option<&str>,
        due_date: Option<Option<&str>>,
        description: Option<Option<&str>>,
        priority: Option<TaskPriority>,
        status: Option<TaskStatus>,
        project_id: Option<Option<i64>>,
    ) -> Result<Self, ValidationError> {
        if title.is_none()
            && due_date.is_none()
            && description.is_none()
            && priority.is_none()
            && status.is_none()
            && project_id.is_none()
        {
            return Err(ValidationError("edit requires a changed field".into()));
        }
        Ok(Self {
            title: title.map(valid_title).transpose()?,
            due_date: due_date
                .map(|date| date.map(valid_due_date).transpose())
                .transpose()?,
            description: description
                .map(|text| text.map(valid_description).transpose())
                .transpose()?,
            priority,
            status,
            project_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_validated() {
        assert_eq!(NewTask::new("  example  ", None).unwrap().title, "example");
        assert!(NewTask::new("   ", None).is_err());
        assert!(NewTask::new(&"a".repeat(501), None).is_err());
        assert!(NewTask::new("ab\0cd", None).is_err());
    }

    #[test]
    fn due_dates_require_real_iso_dates() {
        assert!(NewTask::new("test", Some("2028-02-29")).is_ok());
        for due in ["2027-02-29", "2026-13-01", "2026-1-01", "whatever"] {
            assert!(NewTask::new("test", Some(due)).is_err(), "{due}");
        }
    }

    #[test]
    fn patch_can_clear_a_due_date() {
        let patch = TaskPatch::new(None, Some(None)).unwrap();
        assert_eq!(patch.due_date, Some(None));
        assert!(TaskPatch::new(None, None).is_err());
    }

    #[test]
    fn task_enums_are_validated_and_serialized_as_cli_values() {
        assert_eq!(
            "urgent".parse::<TaskPriority>().unwrap(),
            TaskPriority::Urgent
        );
        assert_eq!(
            "in-progress".parse::<TaskStatus>().unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!("priority".parse::<TaskSort>().unwrap(), TaskSort::Priority);
        assert!("extreme".parse::<TaskPriority>().is_err());
        assert!("doing".parse::<TaskStatus>().is_err());
        assert!("random".parse::<TaskSort>().is_err());
        assert_eq!(
            serde_json::to_string(&TaskStatus::InProgress).unwrap(),
            "\"in-progress\""
        );
    }

    #[test]
    fn task_details_validate_limits_and_support_clearing() {
        assert!(
            NewTask::with_details(
                "test",
                None,
                Some(&"a".repeat(20_001)),
                TaskPriority::Low,
                TaskStatus::Todo
            )
            .is_err()
        );
        assert!(
            NewTask::with_details(
                "test",
                None,
                Some("bad\0text"),
                TaskPriority::Low,
                TaskStatus::Todo
            )
            .is_err()
        );
        assert_eq!(
            TaskPatch::full(None, None, Some(None), None, None, None)
                .unwrap()
                .description,
            Some(None)
        );
        assert!(
            TaskPatch::full(
                None,
                None,
                Some(Some(&"a".repeat(20_001))),
                None,
                None,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn search_filters_validate_dates_and_untrusted_input() {
        assert!(TaskQuery::new(false, None, Some(" "), None, None, None, TaskSort::Due).is_err());
        assert!(
            TaskQuery::new(
                false,
                None,
                Some(&"x".repeat(201)),
                None,
                None,
                None,
                TaskSort::Due
            )
            .is_err()
        );
        assert!(
            TaskQuery::new(
                false,
                None,
                None,
                None,
                None,
                Some("2026-02-30"),
                TaskSort::Due
            )
            .is_err()
        );
        let query = TaskQuery::new(
            false,
            None,
            Some("  report  "),
            None,
            None,
            None,
            TaskSort::Due,
        )
        .unwrap();
        assert_eq!(query.search.as_deref(), Some("report"));
    }
}
