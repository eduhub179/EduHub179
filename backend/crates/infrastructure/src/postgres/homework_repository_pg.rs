//! PostgreSQL implementation of `HomeworkRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_by_id` / `get_by_lesson_instance` rely on primary key and `idx_homeworks_instance_unique`.
//! - `get_files` relies on `idx_homework_files_homework (homework_id, sort_order)`.

use domain::entities::homework::{Homework, HomeworkFile};
use domain::errors::DomainError;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::value_objects::homework_status::HomeworkStatus;
use domain::value_objects::role::UserRole;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL (`homeworks`).
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct HomeworkRow {
    homework_id: Uuid,
    lesson_instance_id: Uuid,
    created_by: Uuid,
    created_by_role: String,   // read as TEXT after explicit cast in SQL
    text_content: Option<String>,
    status: String,            // read as TEXT after explicit cast in SQL
    locked_by_teacher: bool,
    last_edited_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl HomeworkRow {
    /// Converts the database row into a domain `Homework` entity.
    /// Returns `Err` if the role or status strings are invalid (data corruption in DB).
    fn into_domain(self) -> Result<Homework, DomainError> {
        // Parse the DB string into our Value Objects
        let role = UserRole::from_str(&self.created_by_role)
            .map_err(|_| DomainError::InvalidNameFormat)?;
        let status = HomeworkStatus::from_str(&self.status)
            .map_err(|_| DomainError::InvalidHomeworkStatus)?;
        
        // Create the homework entity, then restore the DB-issued `created_at`
        // (try_new stamps the current time, but the row may be older).
        let mut homework = Homework::try_new(
            self.homework_id,
            self.lesson_instance_id,
            self.created_by,
            role,
            self.text_content,
            status,
            self.locked_by_teacher,
            self.last_edited_by,
        )?;
        homework.created_at = self.created_at;
        Ok(homework)
    }
}

/// Internal structure for mapping rows from PostgreSQL (`homework_files`).
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct HomeworkFileRow {
    file_id: Uuid,
    homework_id: Uuid,
    storage_key: String,
    file_name: String,
    mime_type: String,
    size_bytes: i64,
    sort_order: i32,
}

impl HomeworkFileRow {
    /// Converts the database row into a domain `HomeworkFile` entity.
    /// Returns `Err` if the metadata is invalid (data corruption in DB).
    fn into_domain(self) -> Result<HomeworkFile, DomainError> {
        HomeworkFile::try_new(
            self.file_id,
            self.homework_id,
            self.storage_key,
            self.file_name,
            self.mime_type,
            self.size_bytes,
            self.sort_order,
        )
    }
}

/// PostgreSQL-backed implementation of `HomeworkRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct HomeworkRepositoryPg {
    pool: PgPool,
}

impl HomeworkRepositoryPg {
    /// Creates a new repository instance.
    /// Fail-safe: Does not validate the pool connection here;
    /// connection issues will surface on the first query.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    /// This is the single point of error translation, ensuring
    /// business logic never sees database-specific errors.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::HomeworkNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation (idx_homeworks_instance_unique)
                Some("23505") => DomainError::HomeworkAlreadyExists,
                // 23503 = foreign_key_violation. The constraint name tells us WHICH
                // parent record is missing, so the error is placed accurately:
                // - users FK (created_by / last_edited_by) -> the author/editor is gone
                // - lesson_instances FK -> the lesson is gone
                // - homework_files FK (homework_id) -> the owning homework is gone
                Some("23503") => match db_err.constraint() {
                    Some("homeworks_created_by_fkey")
                    | Some("homeworks_last_edited_by_fkey") => DomainError::UserNotFound,
                    _ => DomainError::HomeworkNotFound,
                },
                _ => DomainError::HomeworkNotFound,
            },
            _ => DomainError::HomeworkNotFound,
        }
    }
}

#[async_trait::async_trait]
impl HomeworkRepository for HomeworkRepositoryPg {
    /// Fetches a homework by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, homework_id: Uuid) -> Result<Homework, DomainError> {
        let row = sqlx::query_as::<_, HomeworkRow>(
            r#"
            SELECT homework_id, lesson_instance_id, created_by, created_by_role::TEXT AS created_by_role,
                   text_content, status::TEXT AS status, locked_by_teacher, last_edited_by, created_at
            FROM homeworks
            WHERE homework_id = $1
            "#,
        )
        .bind(homework_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        row.into_domain()
    }

    /// Fetches homework by lesson instance ID.
    /// Performance: Uses `idx_homeworks_instance_unique` (O(log n)).
    async fn get_by_lesson_instance(&self, lesson_instance_id: Uuid) -> Result<Homework, DomainError> {
        let row = sqlx::query_as::<_, HomeworkRow>(
            r#"
            SELECT homework_id, lesson_instance_id, created_by, created_by_role::TEXT AS created_by_role,
                   text_content, status::TEXT AS status, locked_by_teacher, last_edited_by, created_at
            FROM homeworks
            WHERE lesson_instance_id = $1
            "#,
        )
        .bind(lesson_instance_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        row.into_domain()
    }

    /// Fetches all files for a homework, sorted by sort_order then file_id.
    /// Performance: Uses `idx_homework_files_homework (homework_id, sort_order)`.
    async fn get_files(&self, homework_id: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
        let rows = sqlx::query_as::<_, HomeworkFileRow>(
            r#"
            SELECT file_id, homework_id, storage_key, file_name, mime_type, size_bytes, sort_order
            FROM homework_files
            WHERE homework_id = $1
            ORDER BY sort_order, file_id
            "#,
        )
        .bind(homework_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        rows.into_iter().map(HomeworkFileRow::into_domain).collect()
    }

    /// Saves or updates a homework.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a homework with the same `homework_id` exists, it updates the mutable fields.
    /// If a homework with the same `lesson_instance_id` exists (but different `homework_id`),
    /// it raises a unique violation, mapped to `DomainError::HomeworkAlreadyExists`.
    ///
    /// Note: `lesson_instance_id`, `created_by`, `created_by_role`, `created_at` are
    /// immutable after creation (deliberately omitted from the UPDATE list;
    /// `created_at` is written on INSERT only). `updated_at` is maintained by the trigger.
    async fn save(&self, homework: Homework) -> Result<Homework, DomainError> {
        let role_str = homework.created_by_role.to_string();
        let status_str = homework.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO homeworks (homework_id, lesson_instance_id, created_by, created_by_role, text_content, status, locked_by_teacher, last_edited_by, created_at)
            VALUES ($1, $2, $3, $4::user_role, $5, $6::homework_status, $7, $8, $9)
            ON CONFLICT (homework_id) DO UPDATE SET
                text_content = EXCLUDED.text_content,
                status = EXCLUDED.status,
                locked_by_teacher = EXCLUDED.locked_by_teacher,
                last_edited_by = EXCLUDED.last_edited_by,
                updated_at = NOW()
            "#,
        )
        .bind(homework.id)
        .bind(homework.lesson_instance_id)
        .bind(homework.created_by)
        .bind(&role_str)
        .bind(homework.text_content.as_deref())
        .bind(&status_str)
        .bind(homework.locked_by_teacher)
        .bind(homework.last_edited_by)
        .bind(homework.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        Ok(homework)
    }
    
    /// Adds a file to a homework.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for idempotent upsert.
    /// If the homework does not exist, the FK constraint raises a violation,
    /// mapped to `DomainError::HomeworkNotFound`.
    async fn add_file(&self, file: HomeworkFile) -> Result<HomeworkFile, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO homework_files (file_id, homework_id, storage_key, file_name, mime_type, size_bytes, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (file_id) DO UPDATE SET
                homework_id = EXCLUDED.homework_id,
                storage_key = EXCLUDED.storage_key,
                file_name = EXCLUDED.file_name,
                mime_type = EXCLUDED.mime_type,
                size_bytes = EXCLUDED.size_bytes,
                sort_order = EXCLUDED.sort_order
            "#,
        )
        .bind(file.id)
        .bind(file.homework_id)
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

    /// Removes a file by ID.
    /// Returns `HomeworkFileNotFound` if no row was affected (explicit contract).
    async fn remove_file(&self, file_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM homework_files WHERE file_id = $1
            "#,
        )
        .bind(file_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        if result.rows_affected() == 0 {
            return Err(DomainError::HomeworkFileNotFound);
        }
        
        Ok(())
    }

    /// Deletes a homework by ID.
    /// Files are deleted via `ON DELETE CASCADE` FK.
    /// Returns `HomeworkNotFound` if no row was affected.
    async fn delete(&self, homework_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM homeworks WHERE homework_id = $1
            "#,
        )
        .bind(homework_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        
        if result.rows_affected() == 0 {
            return Err(DomainError::HomeworkNotFound);
        }
        
        Ok(())
    }

    /// Creates a homework with its files in a single transaction.
    /// 
    /// Fail-safe behavior: Uses a transaction; on any error the whole transaction
    /// is rolled back (no partial homework/files persisted).
    /// 
    /// Plain INSERT (no conflict clause) → duplicate `lesson_instance_id` yields 
    /// `HomeworkAlreadyExists`.
    async fn create_with_files(&self, homework: Homework, files: Vec<HomeworkFile>) -> Result<Homework, DomainError> {
        // Begin transaction
        let mut tx = self.pool.begin().await.map_err(Self::map_db_error)?;
        
        // Insert homework (plain INSERT - no ON CONFLICT)
        let role_str = homework.created_by_role.to_string();
        let status_str = homework.status.to_string();
        
        sqlx::query(
            r#"
            INSERT INTO homeworks (homework_id, lesson_instance_id, created_by, created_by_role, text_content, status, locked_by_teacher, last_edited_by, created_at)
            VALUES ($1, $2, $3, $4::user_role, $5, $6::homework_status, $7, $8, $9)
            "#,
        )
        .bind(homework.id)
        .bind(homework.lesson_instance_id)
        .bind(homework.created_by)
        .bind(&role_str)
        .bind(homework.text_content.as_deref())
        .bind(&status_str)
        .bind(homework.locked_by_teacher)
        .bind(homework.last_edited_by)
        .bind(homework.created_at)
        .execute(&mut *tx)
        .await
        .map_err(Self::map_db_error)?;
        
        // Insert all files
        for file in files {
            sqlx::query(
                r#"
                INSERT INTO homework_files (file_id, homework_id, storage_key, file_name, mime_type, size_bytes, sort_order)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(file.id)
            .bind(file.homework_id)
            .bind(&file.storage_key)
            .bind(&file.file_name)
            .bind(&file.mime_type)
            .bind(file.size_bytes)
            .bind(file.sort_order)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        }
        
        // Commit transaction
        tx.commit().await.map_err(Self::map_db_error)?;
        
        Ok(homework)
    }
}