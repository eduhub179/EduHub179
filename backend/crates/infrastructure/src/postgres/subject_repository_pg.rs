//! PostgreSQL implementation of `SubjectRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses unique indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_all` relies on the unique index `idx_subjects_name` for fast ordered scans.
//! - `save` uses `ON CONFLICT (subject_id)` for atomic upsert.
//! - `get_by_id` uses primary key index (O(log n)).
use domain::entities::subject::Subject;
use domain::errors::DomainError;
use domain::repositories::subject_repository::SubjectRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
/// Contains technical fields (created_at) that are not part of the domain.
#[derive(Debug, sqlx::FromRow)]
struct SubjectRow {
    subject_id: Uuid,
    name: String,
    /// Technical field: when the subject was created.
    /// Not exposed to the domain layer — used only for auditing/logging if needed.
    created_at: chrono::DateTime<chrono::Utc>,
}

impl SubjectRow {
    /// Converts the database row into a domain `Subject` entity.
    /// Returns `Err` if the name is invalid (data corruption in DB).
    ///
    /// Note: `created_at` is intentionally ignored here.
    /// The domain model does not need this technical field for MVP.
    fn into_domain(self) -> Result<Subject, DomainError> {
        Subject::try_new(self.subject_id, self.name)
    }
}

/// PostgreSQL-backed implementation of `SubjectRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct SubjectRepositoryPg {
    pool: PgPool,
}

impl SubjectRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::SubjectNotFound,
            sqlx::Error::Database(db_err) => {
                // PostgreSQL error code "23505" = unique_violation
                // Triggers on idx_subjects_name (name)
                if db_err.code().as_deref() == Some("23505") {
                    DomainError::SubjectAlreadyExists
                } else {
                    // For other DB errors, we could log them here.
                    // For MVP, we treat them as generic "not found" to avoid leaking details.
                    DomainError::SubjectNotFound
                }
            }
            _ => DomainError::SubjectNotFound,
        }
    }
}

#[async_trait::async_trait]
impl SubjectRepository for SubjectRepositoryPg {
    /// Fetches a subject by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, subject_id: Uuid) -> Result<Subject, DomainError> {
        let row = sqlx::query_as::<_, SubjectRow>(
            r#"
            SELECT subject_id, name, created_at
            FROM subjects
            WHERE subject_id = $1
            "#,
        )
            .bind(subject_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    /// Fetches all subjects, sorted alphabetically by name.
    ///
    /// Performance: Uses the unique index `idx_subjects_name` for fast ordered retrieval.
    /// This is ideal for populating dropdown lists in the UI.
    async fn get_all(&self) -> Result<Vec<Subject>, DomainError> {
        let rows = sqlx::query_as::<_, SubjectRow>(
            r#"
            SELECT subject_id, name, created_at
            FROM subjects
            ORDER BY name
            "#,
        )
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        rows.into_iter()
            .map(SubjectRow::into_domain)
            .collect()
    }

    /// Saves or updates a subject.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a subject with the same `subject_id` exists, it updates the name.
    /// If a subject with the same `name` exists (but different `subject_id`),
    /// it raises a unique violation, mapped to `DomainError::SubjectAlreadyExists`.
    async fn save(&self, subject: Subject) -> Result<Subject, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO subjects (subject_id, name)
            VALUES ($1, $2)
            ON CONFLICT (subject_id) DO UPDATE SET
                name = EXCLUDED.name
            "#,
        )
            .bind(subject.id)
            .bind(&subject.name)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        // Return the same subject (it's now persisted).
        // In a stricter design, we could re-fetch to get the exact `created_at`,
        // but for MVP this is sufficient and avoids an extra round-trip.
        Ok(subject)
    }
}