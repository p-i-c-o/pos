//! Project model and input validation. The slug is a stable, unique reference.
//! Renaming a project does not change its slug or task references.

use serde::Serialize;
use std::{error::Error, fmt};

const MAX_NAME_BYTES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Error for ValidationError {}

pub fn validate_name(name: &str) -> Result<String, ValidationError> {
    let value = name.trim();
    if value.is_empty() || value.len() > MAX_NAME_BYTES || value.contains('\0') {
        return Err(ValidationError(format!(
            "project name must be 1 to {MAX_NAME_BYTES} bytes and contain no NUL"
        )));
    }
    Ok(value.to_string())
}

pub fn validate_slug(slug: &str) -> Result<String, ValidationError> {
    if slug.is_empty()
        || slug.len() > 80
        || !slug.is_ascii()
        || slug.starts_with('-')
        || slug.ends_with('-')
        || slug.contains("--")
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(ValidationError(
            "project slug must be 1-80 lowercase ASCII letters, numbers and single hyphens".into(),
        ));
    }
    Ok(slug.to_owned())
}

fn slug_from_name(name: &str) -> Result<String, ValidationError> {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if slug.len() >= 80 {
                return Err(ValidationError(
                    "generated slug exceeds 80 characters; use --slug".into(),
                ));
            }
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    validate_slug(slug.trim_end_matches('-'))
}

impl NewProject {
    pub fn new(name: &str, slug: Option<&str>) -> Result<Self, ValidationError> {
        let name = validate_name(name)?;
        let slug = match slug {
            Some(slug) => validate_slug(slug)?,
            None => slug_from_name(&name)?,
        };
        Ok(Self { name, slug })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_slugs_are_stable_and_normalized() {
        let project = NewProject::new("  University Work!  ", None).unwrap();
        assert_eq!(project.name, "University Work!");
        assert_eq!(project.slug, "university-work");
        assert_eq!(
            NewProject::new("Projet été", Some("projet-ete"))
                .unwrap()
                .slug,
            "projet-ete"
        );
    }

    #[test]
    fn validation_blocks_blank_and_invalid_values() {
        assert!(NewProject::new("", None).is_err());
        assert!(NewProject::new("   ", None).is_err());
        assert!(NewProject::new("🦊", None).is_err());
        assert!(NewProject::new("Project", Some("Bad Slug")).is_err());
        assert!(NewProject::new("Project", Some("../escape")).is_err());
        assert!(NewProject::new("a".repeat(201).as_str(), None).is_err());
    }
}
