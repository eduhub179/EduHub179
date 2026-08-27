//! Plusnik aggregate — problem sheets, tasks, and plus records.
//!
//! This is the "plusnik" subsystem: a "problems × students" matrix where teachers
//! award pluses for solved problems. The aggregate root is `PlusnikSheet`; tasks
//! and records are children that cannot exist without their parent sheet.
//!
//! Invariants:
//! - `PlusnikSheet.name` must be non-empty after trim and ≤ 255 chars.
//! - `PlusnikSheet.lesson_id` and `created_by` are immutable after creation.
//! - `PlusnikTask.task_number` must be non-empty after trim and ≤ 20 chars.
//! - `PlusnikRecord.revoked_at` implies `revoked_by` is `Some` (mirrors DB CHECK
//!   `chk_revoked_has_reviewer`). A revocation without a revoker is invalid.
//! - `PlusnikRecord.granted_at` is set at creation and immutable.
//!
//! Dependencies: `crate::errors::DomainError`, `crate::value_objects::sheet_status`,
//! `chrono`, `uuid`.
//! Guarantees: Entities can only be created via `try_new`, which validates invariants.

use crate::errors::DomainError;
use crate::value_objects::sheet_status::SheetStatus;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

// ============================================================================
// PlusnikSheet — aggregate root
// ============================================================================

/// A problem worksheet tied to a lesson (the aggregate root).
///
/// A sheet is shared across all teachers of the lesson — any teacher can award
/// pluses for problems from a published sheet. The lifecycle is:
/// `Draft` → `Published` → `Archived`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlusnikSheet {
    /// Unique sheet identifier (UUID v4).
    pub id: Uuid,
    /// The lesson this sheet belongs to (immutable after creation).
    pub lesson_id: Uuid,
    /// The teacher who created the sheet (immutable after creation).
    pub created_by: Uuid,
    /// Sheet title, e.g. "Листок 12: Производные" (1–255 chars).
    pub name: String,
    /// Date the sheet was issued to students.
    pub issue_date: NaiveDate,
    /// Optional submission deadline (informational, nothing automatic happens).
    pub deadline: Option<DateTime<Utc>>,
    /// Sheet status: draft / published / archived.
    pub status: SheetStatus,
    /// Creation timestamp (UTC). Provided by the caller; immutable.
    pub created_at: DateTime<Utc>,
}

impl PlusnikSheet {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidPlusnikSheetName)` if `name` is empty
    /// after trimming or exceeds 255 characters.
    /// The trimmed name is stored.
    pub fn try_new(
        id: Uuid,
        lesson_id: Uuid,
        created_by: Uuid,
        name: String,
        issue_date: NaiveDate,
        deadline: Option<DateTime<Utc>>,
        status: SheetStatus,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(DomainError::InvalidPlusnikSheetName);
        }
        if trimmed_name.chars().count() > 255 {
            return Err(DomainError::InvalidPlusnikSheetName);
        }

        Ok(Self {
            id,
            lesson_id,
            created_by,
            name: trimmed_name.to_string(),
            issue_date,
            deadline,
            status,
            created_at,
        })
    }
}

// ============================================================================
// PlusnikTask — child of PlusnikSheet
// ============================================================================

/// A problem within a sheet. Each problem has a short number ("1а", "2б*", "10")
/// and a display order (`sort_order`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlusnikTask {
    /// Unique task identifier (UUID v4).
    pub id: Uuid,
    /// The sheet this task belongs to.
    pub sheet_id: Uuid,
    /// Problem number: "1а", "1б", "2", "3а*", "10" (1–20 chars).
    pub task_number: String,
    /// Display order within the sheet. Updated when problems are
    /// added/removed from the middle.
    pub sort_order: i32,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

impl PlusnikTask {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidTaskNumber)` if `task_number` is empty
    /// after trimming or exceeds 20 characters.
    /// The trimmed value is stored.
    pub fn try_new(
        id: Uuid,
        sheet_id: Uuid,
        task_number: String,
        sort_order: i32,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let trimmed = task_number.trim();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidTaskNumber);
        }
        if trimmed.chars().count() > 20 {
            return Err(DomainError::InvalidTaskNumber);
        }

        Ok(Self {
            id,
            sheet_id,
            task_number: trimmed.to_string(),
            sort_order,
            created_at,
        })
    }
}

// ============================================================================
// PlusnikRecord — child of PlusnikSheet (a "plus" awarded to a student)
// ============================================================================

/// A single "plus" — a record that a student solved a specific problem.
///
/// Revoking does not delete the row; it fills in `revoked_at` and `revoked_by`.
/// This preserves a full audit trail of who awarded and who revoked each plus.
#[derive(Debug, Clone, PartialEq)]
pub struct PlusnikRecord {
    /// Unique record identifier (UUID v4).
    pub id: Uuid,
    /// Student who received the plus.
    pub student_id: Uuid,
    /// Sheet the plus belongs to.
    pub sheet_id: Uuid,
    /// The specific problem the plus was awarded for.
    pub task_id: Uuid,
    /// Teacher who awarded the plus.
    pub granted_by: Uuid,
    /// When the plus was awarded (immutable).
    pub granted_at: DateTime<Utc>,
    /// When the plus was revoked (None = active).
    pub revoked_at: Option<DateTime<Utc>>,
    /// Who revoked the plus (None = active or not yet revoked).
    pub revoked_by: Option<Uuid>,
    /// Optional comment on revocation.
    pub revoke_comment: Option<String>,
}

impl PlusnikRecord {
    /// Constructor for a new (active) plus.
    /// `granted_at` is set by the caller (typically `Utc::now()`).
    /// `revoked_at`, `revoked_by`, `revoke_comment` are `None`.
    pub fn try_new_active(
        id: Uuid,
        student_id: Uuid,
        sheet_id: Uuid,
        task_id: Uuid,
        granted_by: Uuid,
        granted_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            student_id,
            sheet_id,
            task_id,
            granted_by,
            granted_at,
            revoked_at: None,
            revoked_by: None,
            revoke_comment: None,
        })
    }

    /// Constructor for reconstructing a record from DB row data (may be revoked).
    ///
    /// Validates the DB CHECK constraint `chk_revoked_has_reviewer`:
    /// if `revoked_at` is `Some`, `revoked_by` must also be `Some`.
    /// Returns `Err(DomainError::InvalidPlusnikRecord)` if the invariant is violated.
    pub fn try_from_db(
        id: Uuid,
        student_id: Uuid,
        sheet_id: Uuid,
        task_id: Uuid,
        granted_by: Uuid,
        granted_at: DateTime<Utc>,
        revoked_at: Option<DateTime<Utc>>,
        revoked_by: Option<Uuid>,
        revoke_comment: Option<String>,
    ) -> Result<Self, DomainError> {
        if revoked_at.is_some() && revoked_by.is_none() {
            return Err(DomainError::InvalidPlusnikRecord);
        }

        Ok(Self {
            id,
            student_id,
            sheet_id,
            task_id,
            granted_by,
            granted_at,
            revoked_at,
            revoked_by,
            revoke_comment,
        })
    }

    /// Convenience: is this plus currently active (not revoked)?
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain plusnik`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid {
        Uuid::new_v4()
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // --- PlusnikSheet ---

    #[test]
    fn sheet_try_new_valid() {
        let sheet = PlusnikSheet::try_new(
            uid(),
            uid(),
            uid(),
            "Листок 12: Производные".to_string(),
            date(2026, 9, 7),
            None,
            SheetStatus::Draft,
            now(),
        )
        .unwrap();

        assert_eq!(sheet.name, "Листок 12: Производные");
        assert!(sheet.status.is_draft());
    }

    #[test]
    fn sheet_try_new_trims_name() {
        let sheet = PlusnikSheet::try_new(
            uid(),
            uid(),
            uid(),
            "  Sheet 1  ".to_string(),
            date(2026, 9, 7),
            None,
            SheetStatus::Draft,
            now(),
        )
        .unwrap();

        assert_eq!(sheet.name, "Sheet 1");
    }

    #[test]
    fn sheet_try_new_rejects_empty_name() {
        let err = PlusnikSheet::try_new(
            uid(),
            uid(),
            uid(),
            "   ".to_string(),
            date(2026, 9, 7),
            None,
            SheetStatus::Draft,
            now(),
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidPlusnikSheetName);
    }

    #[test]
    fn sheet_try_new_rejects_long_name() {
        let long_name = "x".repeat(256);
        let err = PlusnikSheet::try_new(
            uid(),
            uid(),
            uid(),
            long_name,
            date(2026, 9, 7),
            None,
            SheetStatus::Draft,
            now(),
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidPlusnikSheetName);
    }

    #[test]
    fn sheet_try_new_accepts_max_length() {
        let max_name = "x".repeat(255);
        let sheet = PlusnikSheet::try_new(
            uid(),
            uid(),
            uid(),
            max_name,
            date(2026, 9, 7),
            None,
            SheetStatus::Published,
            now(),
        )
        .unwrap();

        assert_eq!(sheet.name.chars().count(), 255);
    }

    // --- PlusnikTask ---

    #[test]
    fn task_try_new_valid() {
        let task = PlusnikTask::try_new(
            uid(),
            uid(),
            "1а".to_string(),
            0,
            now(),
        )
        .unwrap();

        assert_eq!(task.task_number, "1а");
        assert_eq!(task.sort_order, 0);
    }

    #[test]
    fn task_try_new_trims_number() {
        let task = PlusnikTask::try_new(
            uid(),
            uid(),
            "  2б*  ".to_string(),
            1,
            now(),
        )
        .unwrap();

        assert_eq!(task.task_number, "2б*");
    }

    #[test]
    fn task_try_new_rejects_empty() {
        let err = PlusnikTask::try_new(
            uid(),
            uid(),
            "  ".to_string(),
            0,
            now(),
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidTaskNumber);
    }

    #[test]
    fn task_try_new_rejects_too_long() {
        let long_number = "x".repeat(21);
        let err = PlusnikTask::try_new(
            uid(),
            uid(),
            long_number,
            0,
            now(),
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidTaskNumber);
    }

    #[test]
    fn task_try_new_accepts_max_length() {
        let max_number = "x".repeat(20);
        let task = PlusnikTask::try_new(
            uid(),
            uid(),
            max_number,
            0,
            now(),
        )
        .unwrap();

        assert_eq!(task.task_number.chars().count(), 20);
    }

    // --- PlusnikRecord ---

    #[test]
    fn record_active_is_active() {
        let record = PlusnikRecord::try_new_active(
            uid(),
            uid(),
            uid(),
            uid(),
            uid(),
            now(),
        )
        .unwrap();

        assert!(record.is_active());
        assert!(record.revoked_at.is_none());
        assert!(record.revoked_by.is_none());
    }

    #[test]
    fn record_revoked_is_not_active() {
        let ts = now();
        let record = PlusnikRecord::try_from_db(
            uid(),
            uid(),
            uid(),
            uid(),
            uid(),
            ts,
            Some(ts),
            Some(uid()),
            Some("Wrong problem".to_string()),
        )
        .unwrap();

        assert!(!record.is_active());
    }

    #[test]
    fn record_from_db_rejects_revoked_without_revoker() {
        let ts = now();
        let err = PlusnikRecord::try_from_db(
            uid(),
            uid(),
            uid(),
            uid(),
            uid(),
            ts,
            Some(ts),
            None,
            None,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidPlusnikRecord);
    }

    #[test]
    fn record_from_db_accepts_active_with_no_revoker() {
        let record = PlusnikRecord::try_from_db(
            uid(),
            uid(),
            uid(),
            uid(),
            uid(),
            now(),
            None,
            None,
            None,
        )
        .unwrap();

        assert!(record.is_active());
    }
}
