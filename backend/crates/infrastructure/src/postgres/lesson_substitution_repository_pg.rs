//! PostgreSQL implementation of `LessonSubstitutionRepository`.
//!
//! Performance notes:
//! - `get` / `save` / `delete` use the primary key `instance_id`.
//! - `instance_ids_for_teacher` uses `idx_lesson_substitutions_teacher`.

use chrono::NaiveDate;
use domain::entities::lesson_substitution::LessonSubstitution;
use domain::errors::DomainError;
use domain::repositories::lesson_substitution_repository::LessonSubstitutionRepository;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct SubstitutionRow {
    instance_id: Uuid,
    substitute_teacher_id: Option<Uuid>,
    note: Option<String>,
    updated_by: Uuid,
}

impl SubstitutionRow {
    /// An empty row can't exist (CHECK constraint); it is reported as missing data.
    fn into_domain(self) -> Result<LessonSubstitution, DomainError> {
        LessonSubstitution::try_new(
            self.instance_id,
            self.substitute_teacher_id,
            self.note,
            self.updated_by,
        )?
        .ok_or(DomainError::InternalError)
    }
}

pub struct LessonSubstitutionRepositoryPg {
    pool: PgPool,
}

impl LessonSubstitutionRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::Database(db_err) => match db_err.constraint() {
                Some("lesson_substitutions_instance_id_fkey") => DomainError::LessonInstanceNotFound,
                Some("lesson_substitutions_substitute_teacher_id_fkey")
                | Some("lesson_substitutions_updated_by_fkey") => DomainError::UserNotFound,
                Some(_) if db_err.code().as_deref() == Some("23514") => {
                    DomainError::InvalidSubstitutionNote
                }
                _ => DomainError::InternalError,
            },
            _ => DomainError::InternalError,
        }
    }
}

#[async_trait::async_trait]
impl LessonSubstitutionRepository for LessonSubstitutionRepositoryPg {
    async fn get(&self, instance_id: Uuid) -> Result<Option<LessonSubstitution>, DomainError> {
        let row = sqlx::query_as::<_, SubstitutionRow>(
            r#"
            SELECT instance_id, substitute_teacher_id, note, updated_by
            FROM lesson_substitutions
            WHERE instance_id = $1
            "#,
        )
        .bind(instance_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.map(SubstitutionRow::into_domain).transpose()
    }

    async fn is_extra(&self, instance_id: Uuid) -> Result<bool, DomainError> {
        let is_extra = sqlx::query_scalar::<_, bool>(
            "SELECT is_extra FROM lesson_instances WHERE instance_id = $1",
        )
        .bind(instance_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(is_extra.unwrap_or(false))
    }

    async fn instance_ids_for_teacher(
        &self,
        teacher_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Uuid>, DomainError> {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT ls.instance_id
            FROM lesson_substitutions ls
            JOIN lesson_instances li ON li.instance_id = ls.instance_id
            WHERE ls.substitute_teacher_id = $1
              AND li.lesson_date BETWEEN $2 AND $3
            "#,
        )
        .bind(teacher_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)
    }

    async fn save(&self, substitution: LessonSubstitution) -> Result<LessonSubstitution, DomainError> {
        let row = sqlx::query_as::<_, SubstitutionRow>(
            r#"
            INSERT INTO lesson_substitutions (instance_id, substitute_teacher_id, note, updated_by)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (instance_id) DO UPDATE SET
                substitute_teacher_id = EXCLUDED.substitute_teacher_id,
                note                  = EXCLUDED.note,
                updated_by            = EXCLUDED.updated_by
            RETURNING instance_id, substitute_teacher_id, note, updated_by
            "#,
        )
        .bind(substitution.instance_id)
        .bind(substitution.substitute_teacher_id)
        .bind(&substitution.note)
        .bind(substitution.updated_by)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    async fn delete(&self, instance_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM lesson_substitutions WHERE instance_id = $1")
            .bind(instance_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;
        Ok(())
    }
}
