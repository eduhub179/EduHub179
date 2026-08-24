//! PostgreSQL implementation of `ScheduleWeekRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//!
//! Performance notes:
//! - `get_by_id` uses the primary key (`week_start_date` is the natural PK).
//! - `save` uses `ON CONFLICT (week_start_date)` for atomic upsert.

use chrono::NaiveDate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::errors::DomainError;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::value_objects::week_status::WeekStatus;
use sqlx::PgPool;
use std::str::FromStr;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct ScheduleWeekRow {
    week_start_date: NaiveDate,
    status: String,
    copied_from: Option<NaiveDate>,
}

impl ScheduleWeekRow {
    /// Converts the database row into a domain `ScheduleWeek` entity.
    /// Returns `Err` if the row contains an unknown `status`
    /// (data corruption in DB).
    fn into_domain(self) -> Result<ScheduleWeek, DomainError> {
        let status = WeekStatus::from_str(&self.status)?;
        Ok(ScheduleWeek::new(
            self.week_start_date,
            status,
            self.copied_from,
        ))
    }
}

/// PostgreSQL-backed implementation of `ScheduleWeekRepository`.
pub struct ScheduleWeekRepositoryPg {
    pool: PgPool,
}

impl ScheduleWeekRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::ScheduleWeekNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23503 = foreign_key_violation
                // (schedule_weeks_copied_from_fkey)
                Some("23503") => match db_err.constraint() {
                    Some("schedule_weeks_copied_from_fkey") => DomainError::ScheduleWeekNotFound,
                    _ => DomainError::ScheduleWeekNotFound,
                },
                _ => DomainError::ScheduleWeekNotFound,
            },
            _ => DomainError::ScheduleWeekNotFound,
        }
    }
}

#[async_trait::async_trait]
impl ScheduleWeekRepository for ScheduleWeekRepositoryPg {
    /// Fetches a week by its start date (the natural key).
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, week_start_date: NaiveDate) -> Result<ScheduleWeek, DomainError> {
        let row = sqlx::query_as::<_, ScheduleWeekRow>(
            r#"
            SELECT week_start_date, status::TEXT, copied_from
            FROM schedule_weeks
            WHERE week_start_date = $1
            "#,
        )
        .bind(week_start_date)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches all weeks, most recent first (admin view).
    async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError> {
        let rows = sqlx::query_as::<_, ScheduleWeekRow>(
            r#"
            SELECT week_start_date, status::TEXT, copied_from
            FROM schedule_weeks
            ORDER BY week_start_date DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(ScheduleWeekRow::into_domain)
            .collect()
    }

    /// Saves or updates a week (atomic upsert on `week_start_date`).
    ///
    /// `status` is bound as a string and cast to the `week_status` enum.
    /// Errors: `ScheduleWeekNotFound` when `copied_from`
    /// references a missing week (FK `schedule_weeks_copied_from_fkey`).
    async fn save(&self, week: ScheduleWeek) -> Result<ScheduleWeek, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO schedule_weeks
                (week_start_date, status, copied_from)
            VALUES ($1, $2::week_status, $3)
            ON CONFLICT (week_start_date) DO UPDATE SET
                status      = EXCLUDED.status,
                copied_from = EXCLUDED.copied_from
            "#,
        )
        .bind(week.week_start_date)
        .bind(week.status.to_string())
        .bind(week.copied_from)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(week)
    }
}
