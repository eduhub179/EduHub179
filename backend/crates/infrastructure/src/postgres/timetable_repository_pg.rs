//! PostgreSQL implementation of `TimetableRepository` (admin timetable editing).
//!
//! Each write is one transaction that first takes `pg_advisory_xact_lock(TIMETABLE_LOCK)`:
//! timetable edits run one at a time, and the lock is released on commit or rollback.
//! Lesson rows are locked (`FOR UPDATE`) before their homework is checked, so a homework
//! added at the same moment either is seen or waits for the edit to finish.

use chrono::{NaiveDate, NaiveTime};
use domain::entities::lesson_template::LessonTemplate;
use domain::errors::DomainError;
use domain::repositories::timetable_repository::{SlotSpec, TimetableRepository, WriteCount};
use domain::value_objects::{day_of_week::DayOfWeek, week_parity::WeekParity};
use sqlx::{PgConnection, PgPool};
use std::str::FromStr;
use uuid::Uuid;

/// Advisory lock key shared by all timetable writes ("179" + "TT").
const TIMETABLE_LOCK: i64 = 179_0707;

#[derive(Debug, sqlx::FromRow)]
struct TemplateRow {
    template_id: Uuid,
    lesson_id: Uuid,
    day: String,
    start_time: NaiveTime,
    end_time: NaiveTime,
    parity: String,
    cabinet_id: Option<Uuid>,
    is_active: bool,
}

impl TemplateRow {
    fn into_domain(self) -> Result<LessonTemplate, DomainError> {
        LessonTemplate::try_new(
            self.template_id,
            self.lesson_id,
            DayOfWeek::from_str(&self.day)?,
            self.start_time,
            self.end_time,
            WeekParity::from_str(&self.parity)?,
            self.cabinet_id,
            self.is_active,
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct InstanceRow {
    instance_id: Uuid,
    week_start_date: NaiveDate,
    cabinet_id: Option<Uuid>,
    has_homework: bool,
}

pub struct TimetableRepositoryPg {
    pool: PgPool,
}

impl TimetableRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => match db_err.constraint() {
                    Some("idx_lesson_templates_no_dup") => DomainError::LessonTemplateAlreadyExists,
                    // homeworks.lesson_instance_id is unique
                    Some(c) if c.contains("homework") => DomainError::HomeworkAlreadyExists,
                    _ => DomainError::LessonInstanceAlreadyExists,
                },
                Some("23503") => match db_err.constraint() {
                    Some("lesson_templates_lesson_id_fkey") => DomainError::LessonNotFound,
                    Some(c) if c.ends_with("cabinet_id_fkey") => DomainError::CabinetNotFound,
                    Some("lesson_instances_week_start_date_fkey") => {
                        DomainError::ScheduleWeekNotFound
                    }
                    // the instance still has homework (ON DELETE RESTRICT)
                    _ => DomainError::HomeworkAlreadyExists,
                },
                Some("23514") => DomainError::InvalidLessonTemplateTime,
                _ => DomainError::InternalError,
            },
            _ => DomainError::InternalError,
        }
    }

    async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, DomainError> {
        let mut tx = self.pool.begin().await.map_err(Self::map_db_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(TIMETABLE_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        Ok(tx)
    }

    /// The template with exactly this slot (active or not), locked.
    async fn find_template(
        conn: &mut PgConnection,
        slot: &SlotSpec,
    ) -> Result<Option<LessonTemplate>, DomainError> {
        sqlx::query_as::<_, TemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT AS day, start_time, end_time,
                   parity::TEXT AS parity, cabinet_id, is_active
            FROM lesson_templates
            WHERE lesson_id = $1 AND day = $2::day_of_week
              AND start_time = $3 AND end_time = $4 AND parity = $5::week_parity
            FOR UPDATE
            "#,
        )
        .bind(slot.lesson_id)
        .bind(slot.day.to_string())
        .bind(slot.start_time)
        .bind(slot.end_time)
        .bind(slot.parity.to_string())
        .fetch_optional(&mut *conn)
        .await
        .map_err(Self::map_db_error)?
        .map(TemplateRow::into_domain)
        .transpose()
    }

    /// Inserts or updates a template row.
    async fn save_template(
        conn: &mut PgConnection,
        template: &LessonTemplate,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO lesson_templates
                (template_id, lesson_id, day, start_time, end_time, parity, cabinet_id, is_active)
            VALUES ($1, $2, $3::day_of_week, $4, $5, $6::week_parity, $7, $8)
            ON CONFLICT (template_id) DO UPDATE SET
                cabinet_id = EXCLUDED.cabinet_id,
                is_active  = EXCLUDED.is_active
            "#,
        )
        .bind(template.id)
        .bind(template.lesson_id)
        .bind(template.day.to_string())
        .bind(template.start_time)
        .bind(template.end_time)
        .bind(template.parity.to_string())
        .bind(template.cabinet_id)
        .bind(template.is_active)
        .execute(&mut *conn)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    /// Makes `slot` an active template (same rules as `LessonTemplateRepositoryPg::save`):
    /// reuses a deactivated one with the same slot, refuses a duplicate and an overlap
    /// with another active slot of the course (except the Odd/Even twin pair).
    async fn activate(
        conn: &mut PgConnection,
        slot: &SlotSpec,
    ) -> Result<LessonTemplate, DomainError> {
        let id = match Self::find_template(conn, slot).await? {
            Some(template) if template.is_active => {
                return Err(DomainError::LessonTemplateAlreadyExists)
            }
            Some(template) => template.id,
            None => Uuid::new_v4(),
        };
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
                  AND NOT (parity = 'odd' AND $6::week_parity = 'even')
                  AND NOT (parity = 'even' AND $6::week_parity = 'odd')
            )
            "#,
        )
        .bind(slot.lesson_id)
        .bind(id)
        .bind(slot.day.to_string())
        .bind(slot.start_time)
        .bind(slot.end_time)
        .bind(slot.parity.to_string())
        .fetch_one(&mut *conn)
        .await
        .map_err(Self::map_db_error)?;
        if conflicting {
            return Err(DomainError::LessonTemplateSlotConflict);
        }
        let template = LessonTemplate::try_new(
            id,
            slot.lesson_id,
            slot.day,
            slot.start_time,
            slot.end_time,
            slot.parity,
            slot.cabinet_id,
            true,
        )?;
        Self::save_template(conn, &template).await?;
        Ok(template)
    }

    /// Locks the template's lessons on or after `from`, then lists them by date.
    async fn lessons_from(
        conn: &mut PgConnection,
        template_id: Uuid,
        from: NaiveDate,
    ) -> Result<Vec<InstanceRow>, DomainError> {
        sqlx::query("SELECT 1 FROM lesson_instances WHERE template_id = $1 AND lesson_date >= $2 FOR UPDATE")
            .bind(template_id)
            .bind(from)
            .execute(&mut *conn)
            .await
            .map_err(Self::map_db_error)?;
        sqlx::query_as::<_, InstanceRow>(
            r#"
            SELECT li.instance_id, li.week_start_date, li.cabinet_id,
                   EXISTS (SELECT 1 FROM homeworks h WHERE h.lesson_instance_id = li.instance_id)
                       AS has_homework
            FROM lesson_instances li
            WHERE li.template_id = $1 AND li.lesson_date >= $2
            ORDER BY li.lesson_date
            "#,
        )
        .bind(template_id)
        .bind(from)
        .fetch_all(&mut *conn)
        .await
        .map_err(Self::map_db_error)
    }
}

fn day_offset(day: DayOfWeek) -> i64 {
    i64::from(day.num_days_from_monday())
}

#[async_trait::async_trait]
impl TimetableRepository for TimetableRepositoryPg {
    async fn add_slot(
        &self,
        slot: SlotSpec,
        apply_from: Option<NaiveDate>,
    ) -> Result<(LessonTemplate, WriteCount), DomainError> {
        let mut tx = self.begin().await?;
        let template = Self::activate(&mut tx, &slot).await?;
        let mut count = WriteCount::default();
        if let Some(from) = apply_from {
            let added = sqlx::query(
                r#"
                INSERT INTO lesson_instances
                    (instance_id, template_id, week_start_date, lesson_date, status, cabinet_id)
                SELECT gen_random_uuid(), $1, w.week_start_date, w.week_start_date + $2::INT,
                       'scheduled', $3
                FROM schedule_weeks w
                WHERE w.week_start_date + $2::INT >= $4
                  AND NOT EXISTS (
                      SELECT 1 FROM lesson_instances li
                      WHERE li.template_id = $1 AND li.week_start_date = w.week_start_date
                  )
                "#,
            )
            .bind(template.id)
            .bind(day_offset(template.day) as i32)
            .bind(template.cabinet_id)
            .bind(from)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
            count.changed = added.rows_affected() as usize;
        }
        tx.commit().await.map_err(Self::map_db_error)?;
        Ok((template, count))
    }

    async fn change_slot(
        &self,
        template_id: Uuid,
        slot: SlotSpec,
        apply_from: Option<NaiveDate>,
    ) -> Result<WriteCount, DomainError> {
        let mut tx = self.begin().await?;
        let mut old = sqlx::query_as::<_, TemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT AS day, start_time, end_time,
                   parity::TEXT AS parity, cabinet_id, is_active
            FROM lesson_templates
            WHERE template_id = $1
            FOR UPDATE
            "#,
        )
        .bind(template_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Self::map_db_error)?
        .ok_or(DomainError::LessonTemplateNotFound)?
        .into_domain()?;
        let mut count = WriteCount::default();

        let same_slot = old.lesson_id == slot.lesson_id
            && old.day == slot.day
            && old.start_time == slot.start_time
            && old.end_time == slot.end_time
            && old.parity == slot.parity;
        if same_slot {
            // Only the cabinet changes: the template keeps its id.
            let old_cabinet = old.cabinet_id;
            old.cabinet_id = slot.cabinet_id;
            Self::save_template(&mut tx, &old).await?;
            if let Some(from) = apply_from {
                // A cabinet set for one day by an admin stays.
                let changed = sqlx::query(
                    r#"
                    UPDATE lesson_instances SET cabinet_id = $2, updated_at = NOW()
                    WHERE template_id = $1 AND lesson_date >= $3
                      AND cabinet_id IS NOT DISTINCT FROM $4
                    "#,
                )
                .bind(template_id)
                .bind(slot.cabinet_id)
                .bind(from)
                .bind(old_cabinet)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;
                count.changed = changed.rows_affected() as usize;
            }
            tx.commit().await.map_err(Self::map_db_error)?;
            return Ok(count);
        }

        // Deactivate first: the new slot may overlap the old one.
        old.is_active = false;
        Self::save_template(&mut tx, &old).await?;
        let new = Self::activate(&mut tx, &slot).await?;

        if let Some(from) = apply_from {
            let taken: Vec<NaiveDate> = sqlx::query_scalar(
                "SELECT week_start_date FROM lesson_instances WHERE template_id = $1",
            )
            .bind(new.id)
            .fetch_all(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
            let course_changed = old.lesson_id != new.lesson_id;
            for lesson in Self::lessons_from(&mut tx, old.id, from).await? {
                let date = lesson.week_start_date + chrono::Duration::days(day_offset(new.day));
                if (course_changed && lesson.has_homework)
                    || taken.contains(&lesson.week_start_date)
                    || date < from
                {
                    count.kept += 1;
                    continue;
                }
                let cabinet = if lesson.cabinet_id == old.cabinet_id {
                    new.cabinet_id
                } else {
                    lesson.cabinet_id
                };
                sqlx::query(
                    r#"
                    UPDATE lesson_instances
                    SET template_id = $2, lesson_date = $3, cabinet_id = $4, updated_at = NOW()
                    WHERE instance_id = $1
                    "#,
                )
                .bind(lesson.instance_id)
                .bind(new.id)
                .bind(date)
                .bind(cabinet)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;
                count.changed += 1;
            }
        }
        tx.commit().await.map_err(Self::map_db_error)?;
        Ok(count)
    }

    async fn remove_slot(
        &self,
        template_id: Uuid,
        apply_from: Option<NaiveDate>,
    ) -> Result<WriteCount, DomainError> {
        let mut tx = self.begin().await?;
        let found =
            sqlx::query("UPDATE lesson_templates SET is_active = FALSE WHERE template_id = $1")
                .bind(template_id)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;
        if found.rows_affected() == 0 {
            return Err(DomainError::LessonTemplateNotFound);
        }
        let mut count = WriteCount::default();
        if let Some(from) = apply_from {
            for lesson in Self::lessons_from(&mut tx, template_id, from).await? {
                if lesson.has_homework {
                    sqlx::query(
                        "UPDATE lesson_instances SET status = 'cancelled', updated_at = NOW() WHERE instance_id = $1",
                    )
                    .bind(lesson.instance_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(Self::map_db_error)?;
                    count.kept += 1;
                } else {
                    sqlx::query("DELETE FROM lesson_instances WHERE instance_id = $1")
                        .bind(lesson.instance_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(Self::map_db_error)?;
                    count.changed += 1;
                }
            }
        }
        tx.commit().await.map_err(Self::map_db_error)?;
        Ok(count)
    }

    async fn place_lesson(
        &self,
        slot: SlotSpec,
        week_start_date: NaiveDate,
        date: NaiveDate,
        moved_from: Option<Uuid>,
    ) -> Result<Uuid, DomainError> {
        let mut tx = self.begin().await?;
        if let Some(original) = moved_from {
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status::TEXT FROM lesson_instances WHERE instance_id = $1 FOR UPDATE",
            )
            .bind(original)
            .fetch_optional(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
            if status.as_deref().is_none_or(|status| status == "cancelled") {
                return Err(DomainError::LessonInstanceNotFound);
            }
        }

        let template = match Self::find_template(&mut tx, &slot).await? {
            Some(template) => template,
            // Inactive: a one-off slot is never generated into other weeks.
            None => {
                let template = LessonTemplate::try_new(
                    Uuid::new_v4(),
                    slot.lesson_id,
                    slot.day,
                    slot.start_time,
                    slot.end_time,
                    slot.parity,
                    slot.cabinet_id,
                    false,
                )?;
                Self::save_template(&mut tx, &template).await?;
                template
            }
        };
        // A lesson restored in its regular slot is not new; anything else is.
        let is_extra = !template.is_active;

        let existing: Option<(Uuid, String)> = sqlx::query_as(
            r#"
            SELECT instance_id, status::TEXT FROM lesson_instances
            WHERE template_id = $1 AND week_start_date = $2
            FOR UPDATE
            "#,
        )
        .bind(template.id)
        .bind(week_start_date)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Self::map_db_error)?;
        let instance_id = match existing {
            Some((_, status)) if status != "cancelled" => {
                return Err(DomainError::LessonInstanceAlreadyExists)
            }
            Some((id, _)) => {
                sqlx::query(
                    r#"
                    UPDATE lesson_instances
                    SET status = 'scheduled', cabinet_id = $2, is_extra = $3, updated_at = NOW()
                    WHERE instance_id = $1
                    "#,
                )
                .bind(id)
                .bind(slot.cabinet_id)
                .bind(is_extra)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;
                id
            }
            None => {
                let id = Uuid::new_v4();
                sqlx::query(
                    r#"
                    INSERT INTO lesson_instances
                        (instance_id, template_id, week_start_date, lesson_date, status, cabinet_id, is_extra)
                    VALUES ($1, $2, $3, $4, 'scheduled', $5, $6)
                    "#,
                )
                .bind(id)
                .bind(template.id)
                .bind(week_start_date)
                .bind(date)
                .bind(slot.cabinet_id)
                .bind(is_extra)
                .execute(&mut *tx)
                .await
                .map_err(Self::map_db_error)?;
                id
            }
        };

        if let Some(original) = moved_from {
            sqlx::query(
                "UPDATE lesson_instances SET status = 'cancelled', updated_at = NOW() WHERE instance_id = $1",
            )
            .bind(original)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
            // A restored lesson may already have homework: then both keep theirs.
            sqlx::query(
                r#"
                UPDATE homeworks SET lesson_instance_id = $2
                WHERE lesson_instance_id = $1
                  AND NOT EXISTS (SELECT 1 FROM homeworks WHERE lesson_instance_id = $2)
                "#,
            )
            .bind(original)
            .bind(instance_id)
            .execute(&mut *tx)
            .await
            .map_err(Self::map_db_error)?;
        }
        tx.commit().await.map_err(Self::map_db_error)?;
        Ok(instance_id)
    }

    async fn class_group_ids(&self, class_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT DISTINCT gm.group_id
            FROM group_members gm
            JOIN users u ON u.user_id = gm.student_id
            WHERE u.class_id = $1 AND u.is_active = TRUE
            "#,
        )
        .bind(class_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)
    }
}
