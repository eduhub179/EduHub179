//! Homework entity and related types.
//!
//! Invariants:
//! - `lesson_instance_id` is immutable after creation (DB has `UNIQUE` constraint on it
//!   + `ON DELETE RESTRICT` FKs to users/lesson_instances).
//! - `created_by` and `created_by_role` are immutable after creation (set at creation time).
//! - `text_content` is optional (homework may be file-only). If provided, must be non-empty after trim.
//! - The "at least text or file" rule is enforced at the use-case layer, NOT here —
//!   per migration comment "Content validation ... at the application level".
//! - `locked_by_teacher` is a one-way lock: once true, students can no longer edit submissions.
//! - `last_edited_by` is NULL on creation; author is not counted as an editor (audit trail).
//!
//! Dependencies: Only `crate::errors::DomainError`, `crate::value_objects::role::UserRole`,
//! `crate::value_objects::homework_status::HomeworkStatus`, and `uuid::Uuid`.
//! Guarantees: Instances can only be created via `try_new`, which validates
//! the invariants. This prevents invalid entities from reaching the repository.

use crate::errors::DomainError;
use crate::value_objects::homework_status::HomeworkStatus;
use crate::value_objects::role::UserRole;
use uuid::Uuid;

/// Representation of a homework assignment in the domain model.
///
/// A homework is attached to exactly one `lesson_instance` (concrete lesson on a concrete date).
/// The DB enforces `UNIQUE` on `lesson_instance_id`, guaranteeing at most one homework per lesson.
#[derive(Debug, Clone, PartialEq)]
pub struct Homework {
    /// Unique homework identifier (UUID v4) — corresponds to `homework_id` in DB.
    pub id: Uuid,
    /// The concrete lesson instance this homework belongs to (immutable after creation).
    /// DB has `UNIQUE` constraint + `ON DELETE RESTRICT` FK.
    pub lesson_instance_id: Uuid,
    /// Author (teacher or student) who created this homework — visible to everyone.
    pub created_by: Uuid,
    /// Creator's role at creation time — for fast permission checks without JOIN.
    /// Immutable after creation.
    pub created_by_role: UserRole,
    /// Optional text content. Homework may be file-only (e.g., only a PDF attachment).
    /// If provided, must be non-empty after trimming.
    pub text_content: Option<String>,
    /// Current status: draft | published | archived.
    pub status: HomeworkStatus,
    /// One-way lock by teacher: true => students can no longer edit their submissions.
    pub locked_by_teacher: bool,
    /// Last editor's user ID for audit trail.
    /// NULL on creation; the author is NOT counted as an editor.
    pub last_edited_by: Option<Uuid>,
}

impl Homework {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidHomeworkTextFormat)` if `text_content` is `Some`
    /// and the trimmed value is empty. The trimmed value is stored.
    /// `None` is allowed (file-only homework; "at least text or file" rule is
    /// enforced at the use-case layer, per migration comment).
    /// No validation on UUIDs, booleans, or status (status is already a closed enum).
    pub fn try_new(
        id: Uuid,
        lesson_instance_id: Uuid,
        created_by: Uuid,
        created_by_role: UserRole,
        text_content: Option<String>,
        status: HomeworkStatus,
        locked_by_teacher: bool,
        last_edited_by: Option<Uuid>,
    ) -> Result<Self, DomainError> {
        // Validate text_content if present: trim and reject empty/whitespace-only.
        // Store the trimmed version.
        let validated_text_content = match text_content {
            Some(content) => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    return Err(DomainError::InvalidHomeworkTextFormat);
                }
                Some(trimmed.to_string())
            }
            None => None,
        };

        Ok(Self {
            id,
            lesson_instance_id,
            created_by,
            created_by_role,
            text_content: validated_text_content,
            status,
            locked_by_teacher,
            last_edited_by,
        })
    }
}

/// Representation of a file attached to a homework.
///
/// Files are stored in S3; this entity holds the metadata.
/// DB: `homework_files` table with FK to `homeworks` (ON DELETE CASCADE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeworkFile {
    /// Unique file identifier (UUID v4) — corresponds to `file_id` in DB.
    pub id: Uuid,
    /// Owning homework (FK ON DELETE CASCADE).
    pub homework_id: Uuid,
    /// Path/key in S3, e.g., "homeworks/2026/07/abc123.pdf" (VARCHAR(500)).
    pub storage_key: String,
    /// Original file name for display (VARCHAR(255)).
    pub file_name: String,
    /// MIME type, e.g., "application/pdf", "image/jpeg" (VARCHAR(100)).
    pub mime_type: String,
    /// File size in bytes, must be >= 0 (DB CHECK constraint).
    pub size_bytes: i64,
    /// Display order, default 0.
    pub sort_order: i32,
}

impl HomeworkFile {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Validation rules (matching DB constraints):
    /// - `storage_key`, `file_name`, `mime_type`: trimmed; empty after trim →
    ///   `Err(DomainError::InvalidHomeworkFileFormat)`.
    /// - Length limits: `storage_key` ≤ 500 chars, `file_name` ≤ 255 chars,
    ///   `mime_type` ≤ 100 chars (counting `chars()` like `StudentGroup::try_new`).
    /// - `size_bytes < 0` → `Err(DomainError::InvalidHomeworkFileSize)`.
    /// - `sort_order`: unvalidated (any i32 allowed).
    pub fn try_new(
        id: Uuid,
        homework_id: Uuid,
        storage_key: String,
        file_name: String,
        mime_type: String,
        size_bytes: i64,
        sort_order: i32,
    ) -> Result<Self, DomainError> {
        // Trim and validate string fields
        let trimmed_storage_key = storage_key.trim();
        let trimmed_file_name = file_name.trim();
        let trimmed_mime_type = mime_type.trim();

        if trimmed_storage_key.is_empty()
            || trimmed_file_name.is_empty()
            || trimmed_mime_type.is_empty()
        {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        // Enforce max lengths matching DB VARCHAR constraints (char count, not bytes)
        if trimmed_storage_key.chars().count() > 500
            || trimmed_file_name.chars().count() > 255
            || trimmed_mime_type.chars().count() > 100
        {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        if size_bytes < 0 {
            return Err(DomainError::InvalidHomeworkFileSize);
        }

        Ok(Self {
            id,
            homework_id,
            storage_key: trimmed_storage_key.to_string(),
            file_name: trimmed_file_name.to_string(),
            mime_type: trimmed_mime_type.to_string(),
            size_bytes,
            sort_order,
        })
    }
}