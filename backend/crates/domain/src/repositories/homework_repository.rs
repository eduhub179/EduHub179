//! Repository trait for homework persistence.
//!
//! Dependencies: Only types from `crate::entities::homework` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::homework::{Homework, HomeworkFile};
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the homework storage.
///
/// A homework is attached to exactly one `lesson_instance` (concrete lesson on a concrete date).
/// The DB enforces `UNIQUE` on `lesson_instance_id`, guaranteeing at most one homework per lesson.
/// Files are managed via `homework_files` table with FK `ON DELETE CASCADE`.
///
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait HomeworkRepository: Send + Sync {
    /// Fetches a homework by its unique identifier.
    ///
    /// Fail-safe: Returns `HomeworkNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, homework_id: Uuid) -> Result<Homework, DomainError>;

    /// Fetches a homework by its lesson instance ID.
    ///
    /// Relies on `idx_homeworks_instance_unique` (UNIQUE on `lesson_instance_id`);
    /// exactly one homework per lesson instance.
    /// Fail-safe: Returns `HomeworkNotFound` if no homework exists for the lesson instance.
    async fn get_by_lesson_instance(&self, lesson_instance_id: Uuid) -> Result<Homework, DomainError>;

    /// Fetches all files attached to a homework, sorted by `sort_order` then `file_id`.
    ///
    /// Relies on `idx_homework_files_homework (homework_id, sort_order)`.
    /// Returns empty vec for missing homework (list-method precedent, like `get_member_ids`).
    async fn get_files(&self, homework_id: Uuid) -> Result<Vec<HomeworkFile>, DomainError>;

    /// Saves or updates a homework (atomic upsert on `homework_id`).
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT (homework_id) DO UPDATE`.
    /// `lesson_instance_id`, `created_by`, `created_by_role` are immutable after creation
    /// and are NOT updated on conflict (deliberately excluded from UPDATE list).
    ///
    /// Returns `HomeworkAlreadyExists` if a DIFFERENT homework already holds the same
    /// `lesson_instance_id` (unique violation on `idx_homeworks_instance_unique`).
    async fn save(&self, homework: Homework) -> Result<Homework, DomainError>;

    /// Adds a file to a homework (idempotent upsert on `file_id`).
    ///
    /// Uses `INSERT ... ON CONFLICT (file_id) DO UPDATE`.
    /// FK violation (homework missing) → `HomeworkNotFound`.
    async fn add_file(&self, file: HomeworkFile) -> Result<HomeworkFile, DomainError>;

    /// Removes a file by its ID.
    ///
    /// Deletes by `file_id` from `homework_files`.
    /// Fail-safe: Returns `HomeworkFileNotFound` if no row was affected (explicit contract).
    async fn remove_file(&self, file_id: Uuid) -> Result<(), DomainError>;

    /// Deletes a homework by its ID.
    ///
    /// Files cascade via FK `ON DELETE CASCADE` (no manual cleanup needed).
    /// Fail-safe: Returns `HomeworkNotFound` if no row was affected.
    async fn delete(&self, homework_id: Uuid) -> Result<(), DomainError>;

    /// Creates a homework with its files in a single transaction.
    ///
    /// Usage scenario: bulk creation (master doc §6.2).
    /// Plain INSERT for homework (no ON CONFLICT — creation must fail loudly if the
    /// lesson instance already has homework). Files inserted within the same transaction.
    ///
    /// Returns `HomeworkAlreadyExists` if duplicate `lesson_instance_id`.
    /// On any error the whole transaction rolls back (no partial homework/files).
    async fn create_with_files(
        &self,
        homework: Homework,
        files: Vec<HomeworkFile>,
    ) -> Result<Homework, DomainError>;
}