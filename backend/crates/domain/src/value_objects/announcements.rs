//! Announcement entity.
//!
//! Invariants:
//! - `title` is plain text, 1..=255 chars (no Markdown).
//! - `content` is MarkdownV2 (Telegram-compatible), non-empty after trim.
//! - `created_by` is immutable after creation (audit).
//! - `expires_at` is optional; if present, must be in the future at creation time
//!   (enforced at use-case layer, not here — clocks can drift).
//!
//! Dependencies: `crate::errors::DomainError`, `crate::value_objects::*`, `chrono`, `uuid`.
//! Guarantees: Instances can only be created via `try_new`, which validates invariants.
use crate::errors::DomainError;
use crate::value_objects::announcement_priority::AnnouncementPriority;
use crate::value_objects::announcement_status::AnnouncementStatus;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An announcement targeted at specific classes/groups/users (or everyone).
///
/// The content is stored as MarkdownV2 so the same string can be rendered
/// in Telegram (via `parse_mode=MarkdownV2`) and in the web/mobile client
/// (via a Markdown renderer).
#[derive(Debug, Clone, PartialEq)]
pub struct Announcement {
    pub id: Uuid,
    pub title: String,
    /// MarkdownV2 body. Non-empty after trim.
    pub content: String,
    pub created_by: Uuid,
    pub status: AnnouncementStatus,
    pub priority: AnnouncementPriority,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Announcement {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidAnnouncementTitle)` if `title` is
    /// empty/whitespace-only or longer than 255 chars.
    ///
    /// Returns `Err(DomainError::InvalidAnnouncementContent)` if `content`
    /// is empty/whitespace-only.
    pub fn try_new(
        id: Uuid,
        title: String,
        content: String,
        created_by: Uuid,
        status: AnnouncementStatus,
        priority: AnnouncementPriority,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let trimmed_title = title.trim();
        if trimmed_title.is_empty() || trimmed_title.chars().count() > 255 {
            return Err(DomainError::InvalidAnnouncementTitle);
        }
        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            return Err(DomainError::InvalidAnnouncementContent);
        }
        Ok(Self {
            id,
            title: trimmed_title.to_string(),
            content: trimmed_content.to_string(),
            created_by,
            status,
            priority,
            expires_at,
            created_at,
        })
    }
}

/// A file attached to an announcement (photo, PDF, document, etc.).
///
/// Files live in object storage (S3/MinIO); this entity holds only metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnouncementFile {
    pub id: Uuid,
    pub announcement_id: Uuid,
    pub storage_key: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sort_order: i32,
}

impl AnnouncementFile {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Validation rules (matching DB constraints):
    /// - `storage_key`, `file_name`, `mime_type`: trimmed; empty after trim →
    ///   `Err(DomainError::InvalidAnnouncementFileFormat)`.
    /// - Length limits: `storage_key` ≤ 500 chars, `file_name` ≤ 255 chars,
    ///   `mime_type` ≤ 100 chars.
    /// - `size_bytes < 0` → `Err(DomainError::InvalidAnnouncementFileSize)`.
    pub fn try_new(
        id: Uuid,
        announcement_id: Uuid,
        storage_key: String,
        file_name: String,
        mime_type: String,
        size_bytes: i64,
        sort_order: i32,
    ) -> Result<Self, DomainError> {
        let trimmed_storage_key = storage_key.trim();
        let trimmed_file_name = file_name.trim();
        let trimmed_mime_type = mime_type.trim();
        if trimmed_storage_key.is_empty()
            || trimmed_file_name.is_empty()
            || trimmed_mime_type.is_empty()
        {
            return Err(DomainError::InvalidAnnouncementFileFormat);
        }
        if trimmed_storage_key.chars().count() > 500
            || trimmed_file_name.chars().count() > 255
            || trimmed_mime_type.chars().count() > 100
        {
            return Err(DomainError::InvalidAnnouncementFileFormat);
        }
        if size_bytes < 0 {
            return Err(DomainError::InvalidAnnouncementFileSize);
        }
        Ok(Self {
            id,
            announcement_id,
            storage_key: trimmed_storage_key.to_string(),
            file_name: trimmed_file_name.to_string(),
            mime_type: trimmed_mime_type.to_string(),
            size_bytes,
            sort_order,
        })
    }
}
