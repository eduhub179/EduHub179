//! PostgreSQL implementation of `AnnouncementRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Visibility query respects all three targeting dimensions + expiry.
//!
//! Performance notes:
//! - `get_by_id` uses the primary key index (O(log n)).
//! - `get_visible_for_user` uses `idx_announcements_published` and partial
//!   indexes on targeting tables; the EXISTS subqueries short-circuit on first match.
//! - `get_by_creator` uses `idx_announcements_created_by`.
//! - `save` uses `ON CONFLICT (announcement_id)` for atomic upsert.
//! - `set_targets` runs in a single transaction: DELETE + 3× INSERT (UNNEST).
//! - `get_files` relies on `idx_announcement_files_announcement`.
use chrono::{DateTime, Utc};
use domain::entities::announcement::{Announcement, AnnouncementFile};
use domain::errors::DomainError;
use domain::repositories::announcement_repository::AnnouncementRepository;
use domain::value_objects::announcement_priority::AnnouncementPriority;
use domain::value_objects::announcement_status::AnnouncementStatus;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// Row mappers
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct AnnouncementRow {
    announcement_id: Uuid,
    title: Option<String>,
    content: Option<String>,
    created_by: Uuid,
    status: String,
    priority: String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl AnnouncementRow {
    fn into_domain(self) -> Result<Announcement, DomainError> {
        let status = AnnouncementStatus::from_str(&self.status)?;
        let priority = AnnouncementPriority::from_str(&self.priority)?;
        Announcement::try_new(
            self.announcement_id,
            self.title,
            self.content,
            self.created_by,
            status,
            priority,
            self.expires_at,
            self.created_at,
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AnnouncementFileRow {
    file_id: Uuid,
    announcement_id: Uuid,
    storage_key: String,
    file_name: String,
    mime_type: String,
    size_bytes: i64,
    sort_order: i32,
}

impl AnnouncementFileRow {
    fn into_domain(self) -> Result<AnnouncementFile, DomainError> {
        AnnouncementFile::try_new(
            self.file_id,
            self.announcement_id,
            self.storage_key,
            self.file_name,
            self.mime_type,
            self.size_bytes,
            self.sort_order,
        )
    }
}

// ============================================================================
// Repository implementation
// ============================================================================

pub struct AnnouncementRepositoryPg {
    pool: PgPool,
}

impl AnnouncementRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    ///
    /// Constraint violations mapped by constraint name:
    /// - `announcements_created_by_fkey` → `UserNotFound`
    /// - `announcement_files_announcement_id_fkey` → `AnnouncementNotFound`
    /// - `announcement_classes_class_id_fkey` → `ClassNotFound`
    /// - `announcement_groups_group_id_fkey` → `StudentGroupNotFound`
    /// - `announcement_users_user_id_fkey` → `UserNotFound`
    /// - `announcement_classes_announcement_id_fkey` /
    ///   `announcement_groups_announcement_id_fkey` /
    ///   `announcement_users_announcement_id_fkey` → `AnnouncementNotFound`
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::AnnouncementNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23503 = foreign_key_violation
                Some("23503") => match db_err.constraint() {
                    Some("announcements_created_by_fkey") => DomainError::UserNotFound,
                    Some("announcement_files_announcement_id_fkey") => {
                        DomainError::AnnouncementNotFound
                    }
                    Some("announcement_classes_announcement_id_fkey")
                    | Some("announcement_groups_announcement_id_fkey")
                    | Some("announcement_users_announcement_id_fkey") => {
                        DomainError::AnnouncementNotFound
                    }
                    Some("announcement_classes_class_id_fkey") => DomainError::ClassNotFound,
                    Some("announcement_groups_group_id_fkey") => DomainError::StudentGroupNotFound,
                    Some("announcement_users_user_id_fkey") => DomainError::UserNotFound,
                    _ => DomainError::AnnouncementNotFound,
                },
                // 23514 = check_violation (size_bytes >= 0)
                Some("23514") => DomainError::InvalidAnnouncementFileSize,
                _ => DomainError::AnnouncementNotFound,
            },
            _ => DomainError::AnnouncementNotFound,
        }
    }

    fn map_db_error_file(err: sqlx::Error) -> DomainError {
        let mapped = Self::map_db_error(err);
        if matches!(mapped, DomainError::AnnouncementNotFound) {
            DomainError::AnnouncementFileNotFound
        } else {
            mapped
        }
    }
}

#[async_trait::async_trait]
impl AnnouncementRepository for AnnouncementRepositoryPg {
    // ========================================================================
    // Announcement CRUD
    // ========================================================================

    async fn get_by_id(&self, announcement_id: Uuid) -> Result<Announcement, DomainError> {
        let row = sqlx::query_as::<_, AnnouncementRow>(
            r#"
            SELECT announcement_id, title, content, created_by,
                   status::TEXT AS status, priority::TEXT AS priority,
                   expires_at, created_at
            FROM announcements
            WHERE announcement_id = $1
            "#,
        )
        .bind(announcement_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Visibility query: returns all announcements the user is allowed to see.
    ///
    /// Rules:
    /// - status = 'published'
    /// - expires_at IS NULL OR expires_at > NOW()
    /// - If no targeting rows exist → visible to everyone
    /// - Otherwise user must match at least one target (class / group / direct)
    ///
    /// Ordering: urgent → high → normal → low, then by created_at DESC.
    async fn get_visible_for_user(&self, user_id: Uuid) -> Result<Vec<Announcement>, DomainError> {
        let rows = sqlx::query_as::<_, AnnouncementRow>(
            r#"
            SELECT a.announcement_id, a.title, a.content, a.created_by,
                   a.status::TEXT AS status, a.priority::TEXT AS priority,
                   a.expires_at, a.created_at
            FROM announcements a
            WHERE a.status = 'published'
              AND (a.expires_at IS NULL OR a.expires_at > NOW())
              AND (
                  -- No targeting rows → visible to everyone
                  NOT EXISTS (
                      SELECT 1 FROM announcement_classes WHERE announcement_id = a.announcement_id
                      UNION ALL
                      SELECT 1 FROM announcement_groups WHERE announcement_id = a.announcement_id
                      UNION ALL
                      SELECT 1 FROM announcement_users WHERE announcement_id = a.announcement_id
                  )
                  -- OR user is explicitly targeted
                  OR EXISTS (
                      SELECT 1 FROM announcement_users
                      WHERE announcement_id = a.announcement_id AND user_id = $1
                  )
                  -- OR user's class is targeted
                  OR EXISTS (
                      SELECT 1 FROM announcement_classes ac
                      WHERE ac.announcement_id = a.announcement_id
                        AND ac.class_id = (SELECT class_id FROM users WHERE user_id = $1)
                  )
                  -- OR user is member of targeted group
                  OR EXISTS (
                      SELECT 1 FROM announcement_groups ag
                      JOIN group_members gm ON gm.group_id = ag.group_id
                      WHERE ag.announcement_id = a.announcement_id
                        AND gm.student_id = $1
                  )
              )
            ORDER BY
                CASE a.priority
                    WHEN 'urgent' THEN 0
                    WHEN 'high'   THEN 1
                    WHEN 'normal' THEN 2
                    WHEN 'low'    THEN 3
                END,
                a.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(AnnouncementRow::into_domain).collect()
    }

    async fn get_by_creator(&self, created_by: Uuid) -> Result<Vec<Announcement>, DomainError> {
        let rows = sqlx::query_as::<_, AnnouncementRow>(
            r#"
            SELECT announcement_id, title, content, created_by,
                   status::TEXT AS status, priority::TEXT AS priority,
                   expires_at, created_at
            FROM announcements
            WHERE created_by = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(created_by)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(AnnouncementRow::into_domain).collect()
    }

    async fn save(&self, announcement: Announcement) -> Result<Announcement, DomainError> {
        let status_str = announcement.status.to_string();
        let priority_str = announcement.priority.to_string();
        sqlx::query(
            r#"
            INSERT INTO announcements
                (announcement_id, title, content, created_by, status, priority, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5::announcement_status, $6::announcement_priority, $7, $8)
            ON CONFLICT (announcement_id) DO UPDATE SET
                title      = EXCLUDED.title,
                content    = EXCLUDED.content,
                status     = EXCLUDED.status,
                priority   = EXCLUDED.priority,
                expires_at = EXCLUDED.expires_at,
                updated_at = NOW()
            "#,
        )
        .bind(announcement.id)
        .bind(announcement.title.as_deref())
        .bind(announcement.content.as_deref())
        .bind(announcement.created_by)
        .bind(&status_str)
        .bind(&priority_str)
        .bind(announcement.expires_at)
        .bind(announcement.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(announcement)
    }

    async fn delete(&self, announcement_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM announcements WHERE announcement_id = $1")
            .bind(announcement_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::AnnouncementNotFound);
        }
        Ok(())
    }

    // ========================================================================
    // Targeting
    // ========================================================================

    /// Replaces targeting lists in a single transaction.
    ///
    /// Uses UNNEST for efficient bulk inserts. Empty slices clear the dimension.
    async fn set_targets(
        &self,
        announcement_id: Uuid,
        class_ids: &[Uuid],
        group_ids: &[Uuid],
        user_ids: &[Uuid],
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(Self::map_db_error)?;

        // Delete existing targeting rows for this announcement.
        sqlx::query("DELETE FROM announcement_classes WHERE announcement_id = $1")
            .bind(announcement_id)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        sqlx::query("DELETE FROM announcement_groups WHERE announcement_id = $1")
            .bind(announcement_id)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        sqlx::query("DELETE FROM announcement_users WHERE announcement_id = $1")
            .bind(announcement_id)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;

        // Insert new targeting rows (skip empty slices to avoid UNNEST-of-empty issues).
        if !class_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO announcement_classes (announcement_id, class_id)
                SELECT $1, unnest($2::uuid[])
                "#,
            )
            .bind(announcement_id)
            .bind(class_ids)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        }
        if !group_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO announcement_groups (announcement_id, group_id)
                SELECT $1, unnest($2::uuid[])
                "#,
            )
            .bind(announcement_id)
            .bind(group_ids)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        }
        if !user_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO announcement_users (announcement_id, user_id)
                SELECT $1, unnest($2::uuid[])
                "#,
            )
            .bind(announcement_id)
            .bind(user_ids)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        }

        tx.commit().await.map_err(Self::map_db_error)?;
        Ok(())
    }

    // ========================================================================
    // Files
    // ========================================================================

    async fn get_files(&self, announcement_id: Uuid) -> Result<Vec<AnnouncementFile>, DomainError> {
        let rows = sqlx::query_as::<_, AnnouncementFileRow>(
            r#"
            SELECT file_id, announcement_id, storage_key, file_name,
                   mime_type, size_bytes, sort_order
            FROM announcement_files
            WHERE announcement_id = $1
            ORDER BY sort_order, file_id
            "#,
        )
        .bind(announcement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(AnnouncementFileRow::into_domain)
            .collect()
    }

    async fn add_file(&self, file: AnnouncementFile) -> Result<AnnouncementFile, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO announcement_files
                (file_id, announcement_id, storage_key, file_name, mime_type, size_bytes, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (file_id) DO UPDATE SET
                announcement_id = EXCLUDED.announcement_id,
                storage_key     = EXCLUDED.storage_key,
                file_name       = EXCLUDED.file_name,
                mime_type       = EXCLUDED.mime_type,
                size_bytes      = EXCLUDED.size_bytes,
                sort_order      = EXCLUDED.sort_order
            "#,
        )
        .bind(file.id)
        .bind(file.announcement_id)
        .bind(&file.storage_key)
        .bind(&file.file_name)
        .bind(&file.mime_type)
        .bind(file.size_bytes)
        .bind(file.sort_order)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(file)
    }

    async fn remove_file(&self, file_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM announcement_files WHERE file_id = $1")
            .bind(file_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error_file)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::AnnouncementFileNotFound);
        }
        Ok(())
    }
}
