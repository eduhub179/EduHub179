//! Repository trait for announcement persistence.
//!
//! Dependencies: Only types from `crate::entities::announcement` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.
use crate::entities::announcement::{Announcement, AnnouncementFile};
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the announcement storage.
///
/// Announcements support flexible targeting:
/// - If all three targeting lists (classes, groups, users) are empty for an
///   announcement, it is visible to **everyone**.
/// - Otherwise, it is visible only to users who match at least one target.
///
/// Files are managed as children of the announcement (cascade on delete).
#[async_trait::async_trait]
pub trait AnnouncementRepository: Send + Sync {
    // ========================================================================
    // Announcement CRUD
    // ========================================================================

    /// Fetches an announcement by its unique identifier.
    /// Fail-safe: Returns `AnnouncementNotFound` if the record doesn't exist.
    async fn get_by_id(&self, announcement_id: Uuid) -> Result<Announcement, DomainError>;

    /// Fetches all announcements visible to the given user.
    ///
    /// Visibility rules:
    /// - Status must be `published`.
    /// - If `expires_at` is set and in the past, the announcement is excluded.
    /// - If targeting lists are empty → visible to everyone.
    /// - Otherwise, the user must match at least one target:
    ///   - Their `class_id` is in `announcement_classes`, OR
    ///   - They are a member of a group in `announcement_groups`, OR
    ///   - Their `user_id` is in `announcement_users`.
    ///
    /// Ordered by (priority DESC, created_at DESC) — urgent first, then recent.
    async fn get_visible_for_user(&self, user_id: Uuid) -> Result<Vec<Announcement>, DomainError>;

    /// Fetches all announcements created by the given user (for the creator's dashboard).
    /// Returns all statuses (draft, published, archived), ordered by created_at DESC.
    async fn get_by_creator(&self, created_by: Uuid) -> Result<Vec<Announcement>, DomainError>;

    /// Saves or updates an announcement (atomic upsert on `announcement_id`).
    ///
    /// Mutable fields (updated on conflict): title, content, status, priority,
    /// expires_at.
    /// IMMUTABLE (excluded from UPDATE list): `created_by`, `created_at`.
    ///
    /// Errors:
    /// - `UserNotFound` — `created_by` references a missing user (FK).
    async fn save(&self, announcement: Announcement) -> Result<Announcement, DomainError>;

    /// Deletes an announcement by ID.
    /// Files cascade via FK `ON DELETE CASCADE`.
    /// Fail-safe: Returns `AnnouncementNotFound` if no row was affected.
    async fn delete(&self, announcement_id: Uuid) -> Result<(), DomainError>;

    // ========================================================================
    // Targeting (N:M relations)
    // ========================================================================

    /// Replaces the targeting lists for an announcement.
    ///
    /// This is a full replace (delete old + insert new), not a merge.
    /// Pass empty slices to clear a targeting dimension.
    ///
    /// If all three slices are empty, the announcement becomes visible to everyone.
    async fn set_targets(
        &self,
        announcement_id: Uuid,
        class_ids: &[Uuid],
        group_ids: &[Uuid],
        user_ids: &[Uuid],
    ) -> Result<(), DomainError>;

    // ========================================================================
    // Files
    // ========================================================================

    /// Fetches all files for an announcement, ordered by `sort_order`.
    /// Returns empty vec for missing announcement (list-method precedent).
    async fn get_files(&self, announcement_id: Uuid) -> Result<Vec<AnnouncementFile>, DomainError>;

    /// Adds a file to an announcement (upsert on `file_id`).
    ///
    /// FK violation (announcement missing) → `AnnouncementNotFound`.
    async fn add_file(&self, file: AnnouncementFile) -> Result<AnnouncementFile, DomainError>;

    /// Removes a file by ID.
    /// Fail-safe: Returns `AnnouncementFileNotFound` if no row was affected.
    async fn remove_file(&self, file_id: Uuid) -> Result<(), DomainError>;
}
