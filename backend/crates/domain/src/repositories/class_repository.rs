//! Repository trait for class persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.
use crate::entities::class::Class;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the class storage.
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait ClassRepository: Send + Sync {
    /// Fetches a class by its unique identifier.
    /// Fail-safe: Returns `ClassNotFound` if the record doesn't exist.
    async fn get_by_id(&self, class_id: Uuid) -> Result<Class, DomainError>;

    /// Fetches all active classes for a specific graduation year, sorted by letter.
    ///
    /// Performance: The implementation in the database should rely on the partial index:
    /// `CREATE INDEX idx_classes_graduation_year ON classes (graduation_year) WHERE is_active = TRUE;`
    async fn get_active_by_year(&self, graduation_year: i32) -> Result<Vec<Class>, DomainError>;

    /// Saves or updates a class.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a class with the same `class_id` exists, it updates mutable fields.
    /// If a class with the same `graduation_year` + `letter` exists,
    /// it raises a unique violation, mapped to `DomainError::InvalidNameFormat`.
    async fn save(&self, class: Class) -> Result<Class, DomainError>;
}