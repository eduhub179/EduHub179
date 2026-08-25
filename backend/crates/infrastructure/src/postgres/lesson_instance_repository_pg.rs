//! PostgreSQL implementation of `LessonInstanceRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//!
//! Performance notes:
//! - `get_by_id` uses the primary key index (O(log n)).
//! - `get_by_week` relies on `idx_lesson_instances_week`.
//! - `get_by_date` relies on `idx_lesson_instances_date`.
//! - `get_by_template` relies on `idx_lesson_instances_template`.
//! - `save` uses `ON CONFLICT (instance_id)` for atomic upsert.

use chrono::NaiveDate;
use domain::entities::lesson_instance::LessonInstance;
use domain::errors::DomainError;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct LessonInstanceRow {
    instance_id: Uuid,
    template_id: Uuid,
    week_start_date: NaiveDate,
    lesson_date: NaiveDate,
    status: String,
    cabinet_id: Option<Uuid>,
}

impl LessonInstanceRow {
    /// Converts the database row into a domain `LessonInstance` entity.
    /// Returns `Err` if the row violates domain invariants (data corruption in DB):
    /// an unknown `status` or a `lesson_date` outside the week.
    fn into_domain(self) -> Result<LessonInstance, DomainError> {
        let status = LessonInstanceStatus::from_str(&self.status)?;
        LessonInstance::try_new(
            self.instance_id,
            self.template_id,
            self.week_start_date,
            self.lesson_date,
            status,
            self.cabinet_id,
        )
    }
}

/// PostgreSQL-backed implementation of `LessonInstanceRepository`.
pub struct LessonInstanceRepositoryPg {
    pool: PgPool,
}

impl LessonInstanceRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::LessonInstanceNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation (idx_lesson_instances_unique)
                Some("23505") => DomainError::LessonInstanceAlreadyExists,
                // 23503 = foreign_key_violation
                // (lesson_instances_template_id_fkey /
                //  lesson_instances_week_start_date_fkey /
                //  lesson_instances_cabinet_id_fkey)
                Some("23503") => match db_err.constraint() {
                    Some("lesson_instances_template_id_fkey") => DomainError::LessonTemplateNotFound,
                    Some("lesson_instances_week_start_date_fkey") => DomainError::ScheduleWeekNotFound,
                    Some("lesson_instances_cabinet_id_fkey") => DomainError::CabinetNotFound,
                    _ => DomainError::LessonInstanceNotFound,
                },
                _ => DomainError::LessonInstanceNotFound,
            },
            _ => DomainError::LessonInstanceNotFound,
        }
    }
}

#[async_trait::async_trait]
impl LessonInstanceRepository for LessonInstanceRepositoryPg {
    /// Fetches an instance by its unique identifier.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, instance_id: Uuid) -> Result<LessonInstance, DomainError> {
        let row = sqlx::query_as::<_, LessonInstanceRow>(
            r#"
            SELECT instance_id, template_id, week_start_date, lesson_date,
                   status::TEXT, cabinet_id
            FROM lesson_instances
            WHERE instance_id = $1
            "#,
        )
        .bind(instance_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches all instances of a week, ordered by (lesson_date, template
    /// start_time) — day first, then time within the day.
    /// Performance: relies on `idx_lesson_instances_week`.
    async fn get_by_week(&self, week_start_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        let rows = sqlx::query_as::<_, LessonInstanceRow>(
            r#"
            SELECT li.instance_id, li.template_id, li.week_start_date, li.lesson_date,
                   li.status::TEXT, li.cabinet_id
            FROM lesson_instances li
                     JOIN lesson_templates lt ON lt.template_id = li.template_id
            WHERE li.week_start_date = $1
            ORDER BY li.lesson_date, lt.start_time
            "#,
        )
        .bind(week_start_date)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonInstanceRow::into_domain)
            .collect()
    }

    /// Fetches all instances on a concrete date, ordered by start_time
    /// (the student schedule backbone).
    /// Performance: relies on `idx_lesson_instances_date`.
    async fn get_by_date(&self, lesson_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        let rows = sqlx::query_as::<_, LessonInstanceRow>(
            r#"
            SELECT li.instance_id, li.template_id, li.week_start_date, li.lesson_date,
                   li.status::TEXT, li.cabinet_id
            FROM lesson_instances li
                     JOIN lesson_templates lt ON lt.template_id = li.template_id
            WHERE li.lesson_date = $1
            ORDER BY lt.start_time
            "#,
        )
        .bind(lesson_date)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonInstanceRow::into_domain)
            .collect()
    }

    /// Fetches all instances generated from a template, ordered by week_start_date.
    /// Performance: relies on `idx_lesson_instances_template`.
    async fn get_by_template(&self, template_id: Uuid) -> Result<Vec<LessonInstance>, DomainError> {
        let rows = sqlx::query_as::<_, LessonInstanceRow>(
            r#"
            SELECT instance_id, template_id, week_start_date, lesson_date,
                   status::TEXT, cabinet_id
            FROM lesson_instances
            WHERE template_id = $1
            ORDER BY week_start_date
            "#,
        )
        .bind(template_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonInstanceRow::into_domain)
            .collect()
    }

    /// Saves or updates an instance (atomic upsert on `instance_id`).
    ///
    /// `status` is bound as a string and cast to the `lesson_instance_status`
    /// enum. Errors: `LessonInstanceAlreadyExists` when a NEW instance
    /// collides with an existing (template_id, week_start_date) pair
    /// (idx_lesson_instances_unique); `LessonTemplateNotFound` / `ScheduleWeekNotFound` /
    /// `CabinetNotFound` when a referenced row is missing (FK violations).
    async fn save(&self, instance: LessonInstance) -> Result<LessonInstance, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO lesson_instances
                (instance_id, template_id, week_start_date, lesson_date, status, cabinet_id)
            VALUES ($1, $2, $3, $4, $5::lesson_instance_status, $6)
            ON CONFLICT (instance_id) DO UPDATE SET
                template_id     = EXCLUDED.template_id,
                week_start_date = EXCLUDED.week_start_date,
                lesson_date     = EXCLUDED.lesson_date,
                status          = EXCLUDED.status,
                cabinet_id      = EXCLUDED.cabinet_id
            "#,
        )
        .bind(instance.id)
        .bind(instance.template_id)
        .bind(instance.week_start_date)
        .bind(instance.lesson_date)
        .bind(instance.status.to_string())
        .bind(instance.cabinet_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(instance)
    }
}
