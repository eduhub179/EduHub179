//! Repository trait for subject persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.
use crate::entities::subject::Subject;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the subject storage (catalog).
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait SubjectRepository: Send + Sync {
    /// Fetches a subject by its unique identifier.
    /// Fail-safe: Returns `SubjectNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, subject_id: Uuid) -> Result<Subject, DomainError>;

    /// Fetches all subjects, sorted alphabetically by name.
    ///
    /// Performance: The implementation should rely on the unique index:
    /// `CREATE UNIQUE INDEX idx_subjects_name ON subjects (name);`
    /// which also serves as a fast scan path for ordered retrieval.
    async fn get_all(&self) -> Result<Vec<Subject>, DomainError>;

    /// Saves or updates a subject.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a subject with the same `subject_id` exists, it updates the name.
    /// If a subject with the same `name` exists (but different `subject_id`),
    /// it raises a unique violation, mapped to `DomainError::SubjectAlreadyExists`.
    async fn save(&self, subject: Subject) -> Result<Subject, DomainError>;
}