//! PostgreSQL implementation of `TimetableRepository` (admin timetable editing).

use chrono::{NaiveDate, NaiveTime};
use domain::entities::lesson_template::LessonTemplate;
use domain::errors::DomainError;
use domain::repositories::timetable_repository::{InstanceRef, TimetableRepository};
use domain::value_objects::{day_of_week::DayOfWeek, week_parity::WeekParity};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

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

#[derive(Debug, sqlx::FromRow)]
struct InstanceRow {
    instance_id: Uuid,
    week_start_date: NaiveDate,
    lesson_date: NaiveDate,
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
                // homeworks.lesson_instance_id is unique; lesson_instances (template, week) too
                Some("23505") if db_err.constraint().is_some_and(|c| c.contains("homework")) => {
                    DomainError::HomeworkAlreadyExists
                }
                Some("23505") => DomainError::LessonInstanceAlreadyExists,
                // 23503: the instance still has homework (ON DELETE RESTRICT)
                Some("23503") => DomainError::HomeworkAlreadyExists,
                _ => DomainError::InternalError,
            },
            _ => DomainError::InternalError,
        }
    }
}

#[async_trait::async_trait]
impl TimetableRepository for TimetableRepositoryPg {
    async fn find_template(
        &self,
        lesson_id: Uuid,
        day: DayOfWeek,
        start_time: NaiveTime,
        end_time: NaiveTime,
        parity: WeekParity,
    ) -> Result<Option<LessonTemplate>, DomainError> {
        let row = sqlx::query_as::<_, TemplateRow>(
            r#"
            SELECT template_id, lesson_id, day::TEXT AS day, start_time, end_time,
                   parity::TEXT AS parity, cabinet_id, is_active
            FROM lesson_templates
            WHERE lesson_id = $1 AND day = $2::day_of_week
              AND start_time = $3 AND end_time = $4 AND parity = $5::week_parity
            "#,
        )
        .bind(lesson_id)
        .bind(day.to_string())
        .bind(start_time)
        .bind(end_time)
        .bind(parity.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.map(|r| {
            LessonTemplate::try_new(
                r.template_id,
                r.lesson_id,
                DayOfWeek::from_str(&r.day)?,
                r.start_time,
                r.end_time,
                WeekParity::from_str(&r.parity)?,
                r.cabinet_id,
                r.is_active,
            )
        })
        .transpose()
    }

    async fn instances_from(
        &self,
        template_id: Uuid,
        from: NaiveDate,
    ) -> Result<Vec<InstanceRef>, DomainError> {
        let rows = sqlx::query_as::<_, InstanceRow>(
            r#"
            SELECT li.instance_id, li.week_start_date, li.lesson_date, li.cabinet_id,
                   EXISTS (SELECT 1 FROM homeworks h WHERE h.lesson_instance_id = li.instance_id)
                       AS has_homework
            FROM lesson_instances li
            WHERE li.template_id = $1 AND li.lesson_date >= $2
            ORDER BY li.lesson_date
            "#,
        )
        .bind(template_id)
        .bind(from)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(rows
            .into_iter()
            .map(|r| InstanceRef {
                instance_id: r.instance_id,
                week_start_date: r.week_start_date,
                lesson_date: r.lesson_date,
                cabinet_id: r.cabinet_id,
                has_homework: r.has_homework,
            })
            .collect())
    }

    async fn repoint_instance(
        &self,
        instance_id: Uuid,
        template_id: Uuid,
        lesson_date: NaiveDate,
        cabinet_id: Option<Uuid>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE lesson_instances
            SET template_id = $2, lesson_date = $3, cabinet_id = $4, updated_at = NOW()
            WHERE instance_id = $1
            "#,
        )
        .bind(instance_id)
        .bind(template_id)
        .bind(lesson_date)
        .bind(cabinet_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    async fn delete_instance(&self, instance_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM lesson_instances WHERE instance_id = $1")
            .bind(instance_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;
        Ok(())
    }

    async fn move_homework(
        &self,
        from_instance: Uuid,
        to_instance: Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE homeworks SET lesson_instance_id = $2 WHERE lesson_instance_id = $1")
            .bind(from_instance)
            .bind(to_instance)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;
        Ok(())
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
