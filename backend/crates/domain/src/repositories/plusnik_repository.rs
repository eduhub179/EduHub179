//! Repository trait for plusnik persistence.
//!
//! The plusnik aggregate root is `PlusnikSheet`. Tasks and records are children
//! that cannot exist without their parent sheet. The repository manages all three
//! through a single trait to maintain aggregate consistency.
//!
//! Dependencies: Only types from `crate::entities::plusnik` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::plusnik::{PlusnikRecord, PlusnikSheet, PlusnikTask};
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with plusnik storage (sheets, tasks, records).
///
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait PlusnikRepository: Send + Sync {
    // ========================================================================
    // Sheet (aggregate root) operations
    // ========================================================================

    /// Fetches a sheet by its unique identifier.
    /// Fail-safe: Returns `PlusnikSheetNotFound` if the record doesn't exist.
    async fn get_sheet_by_id(&self, sheet_id: Uuid) -> Result<PlusnikSheet, DomainError>;

    /// Fetches all sheets for a lesson, ordered by `issue_date` descending.
    /// Returns all statuses (draft, published, archived) — filter in the app layer.
    async fn get_sheets_by_lesson(
        &self,
        lesson_id: Uuid,
    ) -> Result<Vec<PlusnikSheet>, DomainError>;

    /// Fetches all sheets created by a teacher, ordered by `created_at` descending.
    async fn get_sheets_by_creator(
        &self,
        created_by: Uuid,
    ) -> Result<Vec<PlusnikSheet>, DomainError>;

    /// Saves or updates a sheet (atomic upsert on `sheet_id`).
    ///
    /// `lesson_id` and `created_by` are immutable after creation (excluded from UPDATE).
    /// Errors:
    /// - `LessonNotFound` — `lesson_id` references a missing lesson (FK).
    /// - `UserNotFound` — `created_by` references a missing user (FK).
    async fn save_sheet(&self, sheet: PlusnikSheet) -> Result<PlusnikSheet, DomainError>;

    /// Deletes a sheet by ID.
    ///
    /// Tasks cascade via FK `ON DELETE CASCADE` (tasks are deleted with the sheet).
    /// Records have FK `ON DELETE RESTRICT` — deletion fails if any records exist.
    /// Fail-safe: Returns `PlusnikSheetNotFound` if no row was affected.
    /// Returns `PlusnikSheetHasRecords` if records prevent deletion.
    async fn delete_sheet(&self, sheet_id: Uuid) -> Result<(), DomainError>;

    // ========================================================================
    // Task (child) operations
    // ========================================================================

    /// Fetches all tasks for a sheet, ordered by `sort_order`.
    async fn get_tasks(&self, sheet_id: Uuid) -> Result<Vec<PlusnikTask>, DomainError>;

    /// Fetches a task by its unique identifier.
    /// Fail-safe: Returns `PlusnikTaskNotFound` if the record doesn't exist.
    async fn get_task_by_id(&self, task_id: Uuid) -> Result<PlusnikTask, DomainError>;

    /// Adds a task to a sheet (upsert on `task_id`).
    ///
    /// Errors:
    /// - `PlusnikSheetNotFound` — `sheet_id` references a missing sheet (FK).
    /// - `PlusnikTaskAlreadyExists` — duplicate `(sheet_id, task_number)` (unique index).
    async fn save_task(&self, task: PlusnikTask) -> Result<PlusnikTask, DomainError>;

    /// Removes a task by ID.
    ///
    /// Records have FK `ON DELETE RESTRICT` — deletion fails if any records exist.
    /// Fail-safe: Returns `PlusnikTaskNotFound` if no row was affected.
    /// Returns `PlusnikTaskHasRecords` if records prevent deletion.
    async fn delete_task(&self, task_id: Uuid) -> Result<(), DomainError>;

    // ========================================================================
    // Record (child) operations — the "pluses"
    // ========================================================================

    /// Fetches all records for a sheet (the teacher's matrix), including revoked.
    /// Ordered by `granted_at` descending.
    async fn get_records_by_sheet(
        &self,
        sheet_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError>;

    /// Fetches all records for a specific student within a specific sheet,
    /// including revoked. Ordered by `granted_at` descending.
    async fn get_records_by_sheet_and_student(
        &self,
        sheet_id: Uuid,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError>;

    /// Fetches all active (non-revoked) records for a student.
    /// Ordered by `granted_at` descending. Uses `idx_plusnik_records_student_active`.
    async fn get_active_records_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError>;

    /// Fetches all records for a student (including revoked), for history.
    /// Ordered by `granted_at` descending. Uses `idx_plusnik_records_student_all`.
    async fn get_all_records_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError>;

    /// Fetches all active records for a specific task (for statistics).
    /// Ordered by `granted_at` descending. Uses `idx_plusnik_records_task_active`.
    async fn get_active_records_by_task(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError>;

    /// Saves or updates a record (atomic upsert on `record_id`).
    ///
    /// On conflict, updates all mutable fields: `student_id`, `sheet_id`,
    /// `task_id`, `granted_by`, `revoked_at`, `revoked_by`, `revoke_comment`.
    /// `granted_at` is immutable after creation (excluded from UPDATE).
    ///
    /// The DB trigger `check_task_belongs_to_sheet` verifies that `task_id`
    /// belongs to `sheet_id` — if not, returns `TaskNotInSheet`.
    ///
    /// Errors:
    /// - `PlusnikRecordAlreadyExists` — an active plus for this `(student_id, task_id)`
    ///   already exists (partial unique index `idx_plusnik_records_active_unique`).
    /// - `UserNotFound` — `student_id` or `granted_by` references a missing user (FK).
    /// - `PlusnikSheetNotFound` — `sheet_id` references a missing sheet (FK).
    /// - `TaskNotInSheet` — `task_id` does not belong to `sheet_id` (trigger).
    async fn save_record(&self, record: PlusnikRecord) -> Result<PlusnikRecord, DomainError>;

    /// Revokes a plus (sets `revoked_at`, `revoked_by`, `revoke_comment`).
    ///
    /// The DB CHECK `chk_revoked_has_reviewer` requires `revoked_by` when
    /// `revoked_at` is set — the caller must provide `revoked_by`.
    /// Fail-safe: Returns `PlusnikRecordNotFound` if no row was affected.
    async fn revoke_plus(
        &self,
        record_id: Uuid,
        revoked_by: Uuid,
        revoke_comment: Option<String>,
    ) -> Result<(), DomainError>;
}
