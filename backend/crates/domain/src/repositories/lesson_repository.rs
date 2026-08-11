//! Repository trait for lesson persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.
use crate::entities::lesson::Lesson;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the lesson storage.
///
/// A lesson is an abstract (class OR group) + subject. Its teachers are managed
/// through the `lesson_teachers` many-to-many relation, exposed here as well so
/// the whole "lesson aggregate" is accessed through a single repository.
///
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait LessonRepository: Send + Sync {
    /// Fetches a lesson by its unique identifier.
    /// Fail-safe: Returns `LessonNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, lesson_id: Uuid) -> Result<Lesson, DomainError>;

    /// Saves or updates a lesson.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a lesson with the same (class + subject) or (group + subject) exists
    /// but with a different `lesson_id`, it raises a unique violation mapped to
    /// `DomainError::LessonAlreadyExists`
    /// (indexes `idx_lessons_class_unique` / `idx_lessons_group_unique`).
    async fn save(&self, lesson: Lesson) -> Result<Lesson, DomainError>;

    /// Fetches all lessons for a specific class.
    ///
    /// Performance: relies on the partial index `idx_lessons_class`
    /// (class_id) WHERE is_active = TRUE AND class_id IS NOT NULL.
    async fn get_by_class(&self, class_id: Uuid) -> Result<Vec<Lesson>, DomainError>;

    /// Fetches all lessons for a specific student group.
    ///
    /// Performance: relies on the partial index `idx_lessons_group`
    /// (group_id) WHERE is_active = TRUE AND group_id IS NOT NULL.
    async fn get_by_group(&self, group_id: Uuid) -> Result<Vec<Lesson>, DomainError>;

    /// Fetches all lessons taught by a specific teacher (via `lesson_teachers`).
    ///
    /// Performance: relies on `idx_lesson_teachers_teacher`.
    async fn get_by_teacher(&self, teacher_id: Uuid) -> Result<Vec<Lesson>, DomainError>;

    /// Assigns a teacher to a lesson (idempotent).
    ///
    /// Repeated assignment is a no-op.
    /// Fail-safe: Returns `LessonNotFound` if the lesson does not exist.
    async fn assign_teacher(&self, lesson_id: Uuid, teacher_id: Uuid) -> Result<(), DomainError>;

    /// Removes a teacher from a lesson (idempotent).
    ///
    /// Removing a non-assigned teacher is a no-op.
    /// Fail-safe: Returns `LessonNotFound` if the lesson does not exist.
    async fn unassign_teacher(&self, lesson_id: Uuid, teacher_id: Uuid) -> Result<(), DomainError>;

    /// Fetches the IDs of all teachers assigned to a lesson.
    ///
    /// Returns `Vec<Uuid>`
    async fn get_teacher_ids(&self, lesson_id: Uuid) -> Result<Vec<Uuid>, DomainError>;
}
