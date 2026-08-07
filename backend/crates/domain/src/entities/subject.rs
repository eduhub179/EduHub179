//! Subject entity.
//!
//! Invariants:
//! - `name` must be a non-empty string (max 100 chars, enforced by DB).
//! - Subjects act as a global catalog; they are not tied to a specific class.
//!
//! Dependencies: Only `crate::errors::DomainError` and `uuid::Uuid`.
//! Guarantees: An instance can only be created via `try_new`, which validates
//! the name format. This prevents invalid entities from reaching the repository.
use crate::errors::DomainError;
use uuid::Uuid;

/// Representation of a school subject (catalog entry).
///
/// Examples: "Алгебра" (Algebra), "Физика" (Physics), "Информатика" (Informatics).
/// Subjects are referenced by `lessons` to define what is being taught.
/// Unlike `Class`, subjects do not have an `is_active` flag in the current schema;
/// they are managed purely through the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// Unique subject identifier (UUID v4).
    pub id: Uuid,
    /// Subject name (e.g., "Алгебра"/Algebra).
    /// Must be non-empty and max 100 characters (DB constraint).
    pub name: String,
}

impl Subject {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidSubjectNameFormat)` if:
    /// - `name` is empty or contains only whitespace.
    /// - `name` exceeds 100 characters (matches DB `VARCHAR(100)` limit).
    ///
    /// This prevents invalid entities from reaching the repository layer.
    pub fn try_new(id: Uuid, name: String) -> Result<Self, DomainError> {
        let trimmed = name.trim();
        let char_count = trimmed.chars().count();
        if trimmed.is_empty() || char_count > 100{
            return Err(DomainError::InvalidSubjectNameFormat);
        }

        Ok(Self {
            id,
            name: trimmed.to_string(),
        })
    }
}