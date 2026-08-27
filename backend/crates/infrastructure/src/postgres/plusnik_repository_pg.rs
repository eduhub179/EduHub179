//! PostgreSQL implementation of `PlusnikRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses indexes defined in migration 0005 for optimal performance.
//!
//! Performance notes:
//! - `get_sheet_by_id` uses the primary key index.
//! - `get_sheets_by_lesson` uses `idx_plusnik_sheets_lesson` (partial, published only —
//!   but we query all statuses, so this is a seq scan filtered by lesson_id; acceptable
//!   given the expected number of sheets per lesson).
//! - `get_sheets_by_creator` uses `idx_plusnik_sheets_created_by`.
//! - `get_tasks` uses `idx_plusnik_tasks_sheet_order`.
//! - `get_active_records_by_student` uses `idx_plusnik_records_student_active`.
//! - `get_all_records_by_student` uses `idx_plusnik_records_student_all`.
//! - `get_active_records_by_task` uses `idx_plusnik_records_task_active`.
//! - `get_records_by_sheet` uses `idx_plusnik_records_sheet_active` (but we return all,
//!   not just active — full scan on sheet_id, acceptable for a matrix view).

use chrono::{DateTime, NaiveDate, Utc};
use domain::entities::plusnik::{PlusnikRecord, PlusnikSheet, PlusnikTask};
use domain::errors::DomainError;
use domain::repositories::plusnik_repository::PlusnikRepository;
use domain::value_objects::sheet_status::SheetStatus;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// Row mappers
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct PlusnikSheetRow {
    sheet_id: Uuid,
    lesson_id: Uuid,
    created_by: Uuid,
    name: String,
    issue_date: NaiveDate,
    deadline: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
}

impl PlusnikSheetRow {
    fn into_domain(self) -> Result<PlusnikSheet, DomainError> {
        let status = SheetStatus::from_str(&self.status)?;
        PlusnikSheet::try_new(
            self.sheet_id,
            self.lesson_id,
            self.created_by,
            self.name,
            self.issue_date,
            self.deadline,
            status,
            self.created_at,
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PlusnikTaskRow {
    task_id: Uuid,
    sheet_id: Uuid,
    task_number: String,
    sort_order: i32,
    created_at: DateTime<Utc>,
}

impl PlusnikTaskRow {
    fn into_domain(self) -> Result<PlusnikTask, DomainError> {
        PlusnikTask::try_new(
            self.task_id,
            self.sheet_id,
            self.task_number,
            self.sort_order,
            self.created_at,
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PlusnikRecordRow {
    record_id: Uuid,
    student_id: Uuid,
    sheet_id: Uuid,
    task_id: Uuid,
    granted_by: Uuid,
    granted_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<Uuid>,
    revoke_comment: Option<String>,
}

impl PlusnikRecordRow {
    fn into_domain(self) -> Result<PlusnikRecord, DomainError> {
        PlusnikRecord::try_from_db(
            self.record_id,
            self.student_id,
            self.sheet_id,
            self.task_id,
            self.granted_by,
            self.granted_at,
            self.revoked_at,
            self.revoked_by,
            self.revoke_comment,
        )
    }
}

// ============================================================================
// Repository implementation
// ============================================================================

pub struct PlusnikRepositoryPg {
    pool: PgPool,
}

impl PlusnikRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    ///
    /// Constraint violations mapped by constraint name:
    /// - `plusnik_sheets_lesson_id_fkey` → `LessonNotFound`
    /// - `plusnik_sheets_created_by_fkey` → `UserNotFound`
    /// - `plusnik_tasks_sheet_id_fkey` → `PlusnikSheetNotFound`
    /// - `idx_plusnik_tasks_unique` (23505) → `PlusnikTaskAlreadyExists`
    /// - `plusnik_records_student_id_fkey` → `UserNotFound`
    /// - `plusnik_records_sheet_id_fkey` → `PlusnikSheetNotFound`
    /// - `plusnik_records_task_id_fkey` → `PlusnikTaskNotFound`
    /// - `plusnik_records_granted_by_fkey` → `UserNotFound`
    /// - `idx_plusnik_records_active_unique` (23505) → `PlusnikRecordAlreadyExists`
    /// - `chk_revoked_has_reviewer` (414/416) → `InvalidPlusnikRecord`
    ///
    /// The trigger `check_task_belongs_to_sheet` raises a regular PG exception
    /// (P0001 / raise_exception) — detected by the message content.
    fn map_db_err(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::PlusnikSheetNotFound,
            sqlx::Error::Database(db_err) => {
                let code_str = db_err.code();
                let code = code_str.as_deref();
                let constraint = db_err.constraint();

                // Check for trigger exception: "task_id ... does not belong to sheet_id ..."
                // The trigger uses RAISE EXCEPTION with a custom message; sqlx reports
                // it as a regular database error with SQLSTATE P0001 (plpgsql raise).
                let msg = db_err.message();
                if msg.contains("does not belong to sheet_id") {
                    return DomainError::TaskNotInSheet;
                }

                match code {
                    // 23505 = unique_violation
                    Some("23505") => match constraint {
                        Some("idx_plusnik_tasks_unique") => {
                            DomainError::PlusnikTaskAlreadyExists
                        }
                        Some("idx_plusnik_records_active_unique") => {
                            DomainError::PlusnikRecordAlreadyExists
                        }
                        _ => DomainError::PlusnikRecordAlreadyExists,
                    },
                    // 23503 = foreign_key_violation
                    Some("23503") => match constraint {
                        Some("plusnik_sheets_lesson_id_fkey") => DomainError::LessonNotFound,
                        Some("plusnik_sheets_created_by_fkey") => DomainError::UserNotFound,
                        Some("plusnik_tasks_sheet_id_fkey") => DomainError::PlusnikSheetNotFound,
                        Some("plusnik_records_student_id_fkey") => DomainError::UserNotFound,
                        Some("plusnik_records_sheet_id_fkey") => {
                            DomainError::PlusnikSheetNotFound
                        }
                        Some("plusnik_records_task_id_fkey") => DomainError::PlusnikTaskNotFound,
                        Some("plusnik_records_granted_by_fkey") => DomainError::UserNotFound,
                        Some("plusnik_records_revoked_by_fkey") => DomainError::UserNotFound,
                        _ => DomainError::PlusnikSheetNotFound,
                    },
                    // 23001 / 40001 = restrict violation (ON DELETE RESTRICT blocked the delete)
                    Some("23001") | Some("40001") => match constraint {
                        Some("plusnik_records_sheet_id_fkey") => {
                            DomainError::PlusnikSheetHasRecords
                        }
                        Some("plusnik_records_task_id_fkey") => {
                            DomainError::PlusnikTaskHasRecords
                        }
                        _ => DomainError::PlusnikSheetHasRecords,
                    },
                    _ => DomainError::PlusnikSheetNotFound,
                }
            }
            _ => DomainError::PlusnikSheetNotFound,
        }
    }

    /// Maps a DB error for task operations (default not-found = TaskNotFound).
    fn map_db_err_task(err: sqlx::Error) -> DomainError {
        let mapped = Self::map_db_err(err);
        if matches!(mapped, DomainError::PlusnikSheetNotFound) {
            DomainError::PlusnikTaskNotFound
        } else {
            mapped
        }
    }

    /// Maps a DB error for record operations (default not-found = RecordNotFound).
    fn map_db_err_record(err: sqlx::Error) -> DomainError {
        let mapped = Self::map_db_err(err);
        if matches!(mapped, DomainError::PlusnikSheetNotFound) {
            DomainError::PlusnikRecordNotFound
        } else {
            mapped
        }
    }
}

#[async_trait::async_trait]
impl PlusnikRepository for PlusnikRepositoryPg {
    // ========================================================================
    // Sheet operations
    // ========================================================================

    async fn get_sheet_by_id(&self, sheet_id: Uuid) -> Result<PlusnikSheet, DomainError> {
        let row = sqlx::query_as::<_, PlusnikSheetRow>(
            r#"
            SELECT sheet_id, lesson_id, created_by, name, issue_date,
                   deadline, status::TEXT AS status, created_at
            FROM plusnik_sheets
            WHERE sheet_id = $1
            "#,
        )
        .bind(sheet_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        row.into_domain()
    }

    async fn get_sheets_by_lesson(
        &self,
        lesson_id: Uuid,
    ) -> Result<Vec<PlusnikSheet>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikSheetRow>(
            r#"
            SELECT sheet_id, lesson_id, created_by, name, issue_date,
                   deadline, status::TEXT AS status, created_at
            FROM plusnik_sheets
            WHERE lesson_id = $1
            ORDER BY issue_date DESC
            "#,
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter().map(PlusnikSheetRow::into_domain).collect()
    }

    async fn get_sheets_by_creator(
        &self,
        created_by: Uuid,
    ) -> Result<Vec<PlusnikSheet>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikSheetRow>(
            r#"
            SELECT sheet_id, lesson_id, created_by, name, issue_date,
                   deadline, status::TEXT AS status, created_at
            FROM plusnik_sheets
            WHERE created_by = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(created_by)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter().map(PlusnikSheetRow::into_domain).collect()
    }

    async fn save_sheet(&self, sheet: PlusnikSheet) -> Result<PlusnikSheet, DomainError> {
        let status_str = sheet.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO plusnik_sheets
                (sheet_id, lesson_id, created_by, name, issue_date, deadline, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7::sheet_status, $8)
            ON CONFLICT (sheet_id) DO UPDATE SET
                name        = EXCLUDED.name,
                issue_date  = EXCLUDED.issue_date,
                deadline    = EXCLUDED.deadline,
                status      = EXCLUDED.status,
                updated_at  = NOW()
            "#,
        )
        .bind(sheet.id)
        .bind(sheet.lesson_id)
        .bind(sheet.created_by)
        .bind(&sheet.name)
        .bind(sheet.issue_date)
        .bind(sheet.deadline)
        .bind(&status_str)
        .bind(sheet.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        Ok(sheet)
    }

    async fn delete_sheet(&self, sheet_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM plusnik_sheets WHERE sheet_id = $1
            "#,
        )
        .bind(sheet_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::PlusnikSheetNotFound);
        }

        Ok(())
    }

    // ========================================================================
    // Task operations
    // ========================================================================

    async fn get_tasks(&self, sheet_id: Uuid) -> Result<Vec<PlusnikTask>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikTaskRow>(
            r#"
            SELECT task_id, sheet_id, task_number, sort_order, created_at
            FROM plusnik_tasks
            WHERE sheet_id = $1
            ORDER BY sort_order
            "#,
        )
        .bind(sheet_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter().map(PlusnikTaskRow::into_domain).collect()
    }

    async fn get_task_by_id(&self, task_id: Uuid) -> Result<PlusnikTask, DomainError> {
        let row = sqlx::query_as::<_, PlusnikTaskRow>(
            r#"
            SELECT task_id, sheet_id, task_number, sort_order, created_at
            FROM plusnik_tasks
            WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_err_task)?;

        row.into_domain()
    }

    async fn save_task(&self, task: PlusnikTask) -> Result<PlusnikTask, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO plusnik_tasks
                (task_id, sheet_id, task_number, sort_order, created_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (task_id) DO UPDATE SET
                sheet_id    = EXCLUDED.sheet_id,
                task_number = EXCLUDED.task_number,
                sort_order  = EXCLUDED.sort_order
            "#,
        )
        .bind(task.id)
        .bind(task.sheet_id)
        .bind(&task.task_number)
        .bind(task.sort_order)
        .bind(task.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        Ok(task)
    }

    async fn delete_task(&self, task_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM plusnik_tasks WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err_task)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::PlusnikTaskNotFound);
        }

        Ok(())
    }

    // ========================================================================
    // Record operations
    // ========================================================================

    async fn get_records_by_sheet(
        &self,
        sheet_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikRecordRow>(
            r#"
            SELECT record_id, student_id, sheet_id, task_id, granted_by,
                   granted_at, revoked_at, revoked_by, revoke_comment
            FROM plusnik_records
            WHERE sheet_id = $1
            ORDER BY granted_at DESC
            "#,
        )
        .bind(sheet_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter()
            .map(PlusnikRecordRow::into_domain)
            .collect()
    }

    async fn get_records_by_sheet_and_student(
        &self,
        sheet_id: Uuid,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikRecordRow>(
            r#"
            SELECT record_id, student_id, sheet_id, task_id, granted_by,
                   granted_at, revoked_at, revoked_by, revoke_comment
            FROM plusnik_records
            WHERE sheet_id = $1 AND student_id = $2
            ORDER BY granted_at DESC
            "#,
        )
        .bind(sheet_id)
        .bind(student_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter()
            .map(PlusnikRecordRow::into_domain)
            .collect()
    }

    async fn get_active_records_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikRecordRow>(
            r#"
            SELECT record_id, student_id, sheet_id, task_id, granted_by,
                   granted_at, revoked_at, revoked_by, revoke_comment
            FROM plusnik_records
            WHERE student_id = $1 AND revoked_at IS NULL
            ORDER BY granted_at DESC
            "#,
        )
        .bind(student_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter()
            .map(PlusnikRecordRow::into_domain)
            .collect()
    }

    async fn get_all_records_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikRecordRow>(
            r#"
            SELECT record_id, student_id, sheet_id, task_id, granted_by,
                   granted_at, revoked_at, revoked_by, revoke_comment
            FROM plusnik_records
            WHERE student_id = $1
            ORDER BY granted_at DESC
            "#,
        )
        .bind(student_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter()
            .map(PlusnikRecordRow::into_domain)
            .collect()
    }

    async fn get_active_records_by_task(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<PlusnikRecord>, DomainError> {
        let rows = sqlx::query_as::<_, PlusnikRecordRow>(
            r#"
            SELECT record_id, student_id, sheet_id, task_id, granted_by,
                   granted_at, revoked_at, revoked_by, revoke_comment
            FROM plusnik_records
            WHERE task_id = $1 AND revoked_at IS NULL
            ORDER BY granted_at DESC
            "#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_err)?;

        rows.into_iter()
            .map(PlusnikRecordRow::into_domain)
            .collect()
    }

    async fn save_record(&self, record: PlusnikRecord) -> Result<PlusnikRecord, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO plusnik_records
                (record_id, student_id, sheet_id, task_id, granted_by, granted_at,
                 revoked_at, revoked_by, revoke_comment)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (record_id) DO UPDATE SET
                student_id     = EXCLUDED.student_id,
                sheet_id       = EXCLUDED.sheet_id,
                task_id        = EXCLUDED.task_id,
                granted_by     = EXCLUDED.granted_by,
                revoked_at     = EXCLUDED.revoked_at,
                revoked_by     = EXCLUDED.revoked_by,
                revoke_comment = EXCLUDED.revoke_comment
            "#,
        )
        .bind(record.id)
        .bind(record.student_id)
        .bind(record.sheet_id)
        .bind(record.task_id)
        .bind(record.granted_by)
        .bind(record.granted_at)
        .bind(record.revoked_at)
        .bind(record.revoked_by)
        .bind(&record.revoke_comment)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err_record)?;

        Ok(record)
    }

    async fn revoke_plus(
        &self,
        record_id: Uuid,
        revoked_by: Uuid,
        revoke_comment: Option<String>,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE plusnik_records
            SET revoked_at = NOW(),
                revoked_by = $2,
                revoke_comment = $3
            WHERE record_id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(record_id)
        .bind(revoked_by)
        .bind(revoke_comment)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_err_record)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::PlusnikRecordNotFound);
        }

        Ok(())
    }
}
