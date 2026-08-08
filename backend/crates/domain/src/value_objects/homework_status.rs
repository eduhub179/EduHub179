//! Value Object for homework status.
//!
//! Corresponds to the `homework_status` ENUM in PostgreSQL ('draft', 'published', 'archived').
//! Guarantees: An instance can only be created from the predefined set.
//! Any attempt to parse an unknown string from the DB will return `Err(DomainError::InvalidHomeworkStatus)`,
//! preventing "garbage" statuses from entering the system.

use crate::errors::DomainError;
use std::str::FromStr;

/// Homework status in the system.
/// Corresponds to the `homework_status` ENUM in PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HomeworkStatus {
    /// Draft: visible only to the creator (teacher), not to students.
    Draft,
    /// Published: visible to students, they can submit solutions.
    Published,
    /// Archived: hidden from active lists, read-only for history.
    Archived,
}

impl HomeworkStatus {
    /// Fail-safe: Explicit check for Draft status.
    /// Use this instead of `self == HomeworkStatus::Draft` for better readability.
    pub fn is_draft(&self) -> bool {
        matches!(self, HomeworkStatus::Draft)
    }

    /// Fail-safe: Explicit check for Published status.
    pub fn is_published(&self) -> bool {
        matches!(self, HomeworkStatus::Published)
    }

    /// Fail-safe: Explicit check for Archived status.
    pub fn is_archived(&self) -> bool {
        matches!(self, HomeworkStatus::Archived)
    }

    /// Checks if the homework is visible to students.
    /// Only Published homework is visible to students.
    /// Draft is creator-only, Archived is hidden from active lists (per migration comment).
    pub fn is_visible_to_students(&self) -> bool {
        matches!(self, HomeworkStatus::Published)
    }
}

impl FromStr for HomeworkStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(HomeworkStatus::Draft),
            "published" => Ok(HomeworkStatus::Published),
            "archived" => Ok(HomeworkStatus::Archived),
            _ => Err(DomainError::InvalidHomeworkStatus),
        }
    }
}

impl std::fmt::Display for HomeworkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HomeworkStatus::Draft => "draft",
            HomeworkStatus::Published => "published",
            HomeworkStatus::Archived => "archived",
        };
        write!(f, "{}", s)
    }
}