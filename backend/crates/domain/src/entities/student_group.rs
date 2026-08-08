//! StudentGroup entity.
//!
//! Invariants:
//! - `name` must be a non-empty string (max 100 characters, enforced by DB).
//! - Groups are NOT tied to a single class: a group can unite students
//!   from different classes (e.g., "Английский B1" — students from 10а, 10б, 10в).
//!
//! Dependencies: Only `crate::errors::DomainError` and `uuid::Uuid`.
//! Guarantees: An instance can only be created via `try_new`, which validates
//! the name format. This prevents invalid entities from reaching the repository.
use crate::errors::DomainError;
use uuid::Uuid;

/// Representation of a student group.
///
/// Examples: "Английский B1" (English B1), "Информатика базовая" (Basic Informatics).
/// A group is an arbitrary subset of school students that may span multiple classes.
/// Unlike `Class`, groups do not have an `is_active` flag in the current schema;
/// they are managed purely through the catalog + membership (`group_members`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudentGroup {
    /// Unique group identifier (UUID v4).
    pub id: Uuid,
    /// Group name (e.g., "Английский B1"/English B1).
    /// Must be non-empty and max 100 characters (DB `VARCHAR(100)` constraint).
    pub name: String,
}

impl StudentGroup {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidStudentGroupNameFormat)` if:
    /// - `name` is empty or contains only whitespace.
    /// - `name` exceeds 100 **characters** (matches DB `VARCHAR(100)`,
    ///   which counts characters, not bytes — important for Cyrillic).
    ///
    /// Leading/trailing whitespace is trimmed before persisting.
    pub fn try_new(id: Uuid, name: String) -> Result<Self, DomainError> {
        let trimmed = name.trim();
        let char_count = trimmed.chars().count();
        if trimmed.is_empty() || char_count > 100 {
            return Err(DomainError::InvalidStudentGroupNameFormat);
        }
        Ok(Self {
            id,
            name: trimmed.to_string(),
        })
    }
}