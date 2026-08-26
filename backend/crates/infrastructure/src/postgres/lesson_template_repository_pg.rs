//! PostgreSQL implementation of `LessonTemplateRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//!
//! Performance notes:
//! - `get_by_id` uses the primary key index (O(log n)).
//! - `get_active_for_day` relies on `idx_lesson_templates_day_active`.
//! - `save` uses `ON CONFLICT (template_id)` for atomic upsert.
//!
//! Substitutions are handled at the instance level (cancel original + create
//! replacement instance), not by templates.
//!
//! Slot-conflict rule: `save` rejects a NEW or UPDATED active template that
//! would overlap another ACTIVE template of the same lesson at a
//! parity-conflicting slot (Every conflicts with everything; Odd/Odd and
//! Even/Even conflict; Odd/Even twins are the only allowed overlap). The
//! check runs inside the same transaction as the write.

use chrono::NaiveTime;
use domain::entities::lesson_template::LessonTemplate;
use domain::errors::DomainError;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::value_objects::day_of_week::DayOfWeek;
use domain::value_objects::week_parity::WeekParity;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct LessonTemplateRow {
    template_id: Uuid,
    lesson_id: Uuid,
    day: String,
    start_time: NaiveTime,
    end_time: NaiveTime,
    parity: String,
    cabinet_id: Option<Uuid>,
    is_active: bool,
}

impl LessonTemplateRow {
    /// Converts the database row into a domain `LessonTemplate` entity.
    /// Returns `Err` if the row violates domain invariants (data corruption in DB).
    fn into_domain(self) -> Result<LessonTemplate, DomainError> {
        let day = DayOfWeek::from_str(&self.day)?;
        let parity = WeekParity::from_str(&self.parity)?;
        LessonTemplate::try_new(
            self.template_id,
            self.lesson_id,
            day,
            self.start_time,
            self.end_time,
            parity,
            self.cabinet_id,
            self.is_active,
        )
    }
}

/// PostgreSQL-backed implementation of `LessonTemplateRepository`.
pub struct LessonTemplateRepositoryPg {
    pool: PgPool,
}

impl LessonTemplateRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::LessonTemplateNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation (idx_lesson_templates_no_dup)
                Some("23505") => DomainError::LessonTemplateAlreadyExists,
                // 23503 = foreign_key_violation
                // (lesson_templates_lesson_id_fkey / lesson_templates_cabinet_id_fkey)
                Some("23503") => match db_err.constraint() {
                    Some("lesson_templates_lesson_id_fkey") => DomainError::LessonNotFound,
                    Some("lesson_templates_cabinet_id_fkey") => DomainError::CabinetNotFound,
                    _ => DomainError::InvalidLessonTemplateReference,
                },
                _ => DomainError::LessonTemplateNotFound,
            },
            _ => DomainError::LessonTemplateNotFound,
        }
    }
}

#[async_trait::async_trait]
impl LessonTemplateRepository for LessonTemplateRepositoryPg {
    /// Fetches a template by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, template_id: Uuid) -> Result<LessonTemplate, DomainError> {
        let row = sqlx::query_as::<_, LessonTemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT, start_time, end_time,
                   parity::TEXT, cabinet_id, is_active
            FROM lesson_templates
            WHERE template_id = $1
            "#,
        )
        .bind(template_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches ALL templates of a lesson (active and archived),
    /// ordered by (day, start_time).
    async fn get_by_lesson(&self, lesson_id: Uuid) -> Result<Vec<LessonTemplate>, DomainError> {
        let rows = sqlx::query_as::<_, LessonTemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT, start_time, end_time,
                   parity::TEXT, cabinet_id, is_active
            FROM lesson_templates
            WHERE lesson_id = $1
            ORDER BY day, start_time
            "#,
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonTemplateRow::into_domain)
            .collect()
    }

    /// Fetches all ACTIVE templates on a given day, ordered by start_time.
    /// Performance: relies on `idx_lesson_templates_day_active`.
    async fn get_active_for_day(&self, day: DayOfWeek) -> Result<Vec<LessonTemplate>, DomainError> {
        let rows = sqlx::query_as::<_, LessonTemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT, start_time, end_time,
                   parity::TEXT, cabinet_id, is_active
            FROM lesson_templates
            WHERE day = $1::day_of_week
              AND is_active = TRUE
            ORDER BY start_time
            "#,
        )
        .bind(day.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonTemplateRow::into_domain)
            .collect()
    }

    /// Fetches ALL active templates, ordered by (day, start_time).
    async fn get_all_active(&self) -> Result<Vec<LessonTemplate>, DomainError> {
        let rows = sqlx::query_as::<_, LessonTemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT, start_time, end_time,
                   parity::TEXT, cabinet_id, is_active
            FROM lesson_templates
            WHERE is_active = TRUE
            ORDER BY day, start_time
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter()
            .map(LessonTemplateRow::into_domain)
            .collect()
    }

    /// Saves or updates a template (atomic upsert on `template_id`).
    ///
    /// For ACTIVE templates the write is preceded by the slot-conflict check:
    /// another ACTIVE template of the same lesson may not overlap in (day, time)
    /// unless the parities are the Odd/Even twin pair. Exact duplicates are
    /// deliberately excluded here — they fall through to the dedup index
    /// (`idx_lesson_templates_no_dup` → `LessonTemplateAlreadyExists`).
    ///
    /// Note: the check is race-safe against concurrent saves only at READ
    /// COMMITTED level; a serializable guarantee would need an advisory lock.
    /// Acceptable for the single-admin schedule flow.
    async fn save(&self, template: LessonTemplate) -> Result<LessonTemplate, DomainError> {
        const UPSERT: &str = r#"
            INSERT INTO lesson_templates
                (template_id, lesson_id, day, start_time, end_time, parity, cabinet_id, is_active)
            VALUES ($1, $2, $3::day_of_week, $4, $5, $6::week_parity, $7, $8)
            ON CONFLICT (template_id) DO UPDATE SET
                lesson_id   = EXCLUDED.lesson_id,
                day         = EXCLUDED.day,
                start_time  = EXCLUDED.start_time,
                end_time    = EXCLUDED.end_time,
                parity      = EXCLUDED.parity,
                cabinet_id  = EXCLUDED.cabinet_id,
                is_active   = EXCLUDED.is_active
        "#;

        if template.is_active {
            let mut tx = self.pool.begin().await.map_err(Self::map_db_error)?;

            let conflicting: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM lesson_templates
                    WHERE lesson_id = $1
                      AND template_id != $2
                      AND is_active = TRUE
                      AND day = $3::day_of_week
                      AND start_time < $5
                      AND end_time > $4
                      -- exact duplicates are the dedup index's job
                      AND NOT (start_time = $4 AND end_time = $5 AND parity = $6::week_parity)
                      -- Odd/Even twins are the ONLY allowed overlap
                      AND NOT (parity = 'odd' AND $6::week_parity = 'even')
                      AND NOT (parity = 'even' AND $6::week_parity = 'odd')
                )
                "#,
            )
            .bind(template.lesson_id)
            .bind(template.id)
            .bind(template.day.to_string())
            .bind(template.start_time)
            .bind(template.end_time)
            .bind(template.parity.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;

            if conflicting {
                return Err(DomainError::LessonTemplateSlotConflict);
            }

            sqlx::query(UPSERT)
                .bind(template.id)
                .bind(template.lesson_id)
                .bind(template.day.to_string())
                .bind(template.start_time)
                .bind(template.end_time)
                .bind(template.parity.to_string())
                .bind(template.cabinet_id)
                .bind(template.is_active)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;

            tx.commit().await.map_err(Self::map_db_error)?;
        } else {
            // Archiving cannot create conflicts — plain upsert.
            sqlx::query(UPSERT)
                .bind(template.id)
                .bind(template.lesson_id)
                .bind(template.day.to_string())
                .bind(template.start_time)
                .bind(template.end_time)
                .bind(template.parity.to_string())
                .bind(template.cabinet_id)
                .bind(template.is_active)
                .execute(&self.pool)
                .await
                .map_err(Self::map_db_error)?;
        }
        Ok(template)
    }
}
