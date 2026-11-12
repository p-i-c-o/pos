//! Task domain model and input validation. No storage or UI code lives here.

use chrono::NaiveDate;
use serde::Serialize;
use std::{error::Error, fmt};

pub const MAX_TITLE_LEN: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub due_date: Option<String>,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct NewTask {
    pub title: String,
    pub due_date: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskPatch {
    pub title: Option<String>,
    /// None = unchanged; Some(None) = remove due date.
    pub due_date: Option<Option<String>>,
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

impl NewTask {
    pub fn new(title: &str, due_date: Option<&str>) -> Result<Self, ValidationError> {
        Ok(Self {
            title: valid_title(title)?,
            due_date: due_date.map(valid_due_date).transpose()?,
        })
    }
}

impl TaskPatch {
    pub fn new(
        title: Option<&str>,
        due_date: Option<Option<&str>>,
    ) -> Result<Self, ValidationError> {
        if title.is_none() && due_date.is_none() {
            return Err(ValidationError("edit requires a changed field".into()));
        }
        Ok(Self {
            title: title.map(valid_title).transpose()?,
            due_date: due_date
                .map(|new_date| new_date.map(valid_due_date).transpose())
                .transpose()?,
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
}
