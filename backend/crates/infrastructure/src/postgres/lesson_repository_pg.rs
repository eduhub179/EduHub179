//! PostgreSQL implementation of `LessonRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_by_class` relies on the partial index `idx_lessons_class`
//!   (class_id) WHERE is_active = TRUE AND class_id IS NOT NULL.
//! - `get_by_group` relies on the partial index `idx_lessons_group`
//!   (group_id) WHERE is_active = TRUE AND group_id IS NOT NULL.
//! - `get_by_teacher` relies on `idx_lesson_teachers_teacher`.
//! - `save` uses `ON CONFLICT (lesson_id)` for atomic upsert; the partial unique
//!   indexes `idx_lessons_class_unique` / `idx_lessons_group_unique` guard against
//!   duplicate (class/group + subject) combinations.
//! - `assign_teacher` uses `ON CONFLICT (lesson_id, teacher_id) DO NOTHING` (idempotent).
use domain::entities::lesson::Lesson;
use domain::errors::DomainError;
use domain::repositories::lesson_repository::LessonRepository;
use domain::value_objects::lesson_target::LessonTarget;
use sqlx::PgPool;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL (`lessons`).
/// Kept private to isolate database schema from domain model.
///
/// The XOR invariant (exactly one of class_id / group_id) is stored in the DB as
/// two nullable columns guarded by the CHECK constraint `chk_one_entity`.
/// It is reconstructed into the `LessonTarget` enum via `LessonTarget::from_db`.
#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct LessonRow {
    lesson_id: Uuid,
    /// NULL if the lesson targets a group.
    class_id: Option<Uuid>,
    /// NULL if the lesson targets a class.
    group_id: Option<Uuid>,
    subject_id: Uuid,
    is_active: bool,
    /// Technical field: when the lesson was created.
    /// Not exposed to the domain layer — kept for auditing/logging if needed.
    created_at: chrono::DateTime<chrono::Utc>,
}

impl LessonRow {
    /// Converts the database row into a domain `Lesson` entity.
    ///
    /// Returns `Err(LessonNotFound)` if the XOR invariant is violated in the DB
    /// (both class_id and group_id set, or both NULL). This cannot happen while
    /// the CHECK constraint `chk_one_entity` is intact; treating it as "not found"
    /// is the fail-safe choice consistent with `UserRepositoryPg`'s handling of
    /// corrupted rows.
    fn into_domain(self) -> Result<Lesson, DomainError> {
        let target = LessonTarget::from_db(self.class_id, self.group_id)
            .ok_or(DomainError::LessonNotFound)?; // class_id == None && group_id == None =>
                                                  // Lesson doesnt exist
        Ok(Lesson::new(
            self.lesson_id,
            target,
            self.subject_id,
            self.is_active,
        ))
    }
}

/// PostgreSQL-backed implementation of `LessonRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct LessonRepositoryPg {
    pool: PgPool,
}

impl LessonRepositoryPg {
    /// Creates a new repository instance.
    /// Fail-safe: Does not validate the pool connection here;
    /// connection issues will surface on the first query.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies that a lesson with the given ID exists.
    /// Returns `Err(LessonNotFound)` otherwise.
    ///
    /// Used by operations that must explicitly honour the "lesson not found"
    /// contract even when the underlying SQL would not raise an error
    /// (e.g. a bare `DELETE` on a non-existent lesson affects 0 rows silently).
    async fn ensure_lesson_exists(&self, lesson_id: Uuid) -> Result<(), DomainError> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT lesson_id FROM lessons WHERE lesson_id = $1")
                .bind(lesson_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Self::map_db_error)?;
        match row {
            Some(_) => Ok(()),
            None => Err(DomainError::LessonNotFound),
        }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    /// This is the single point of error translation, ensuring
    /// business logic never sees database-specific errors.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::LessonNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation
                // (idx_lessons_class_unique / idx_lessons_group_unique)
                Some("23505") => DomainError::LessonAlreadyExists,
                // 23503 = foreign_key_violation
                // (class, group, subject, or teacher does not exist).
                // The use-case layer is expected to validate those beforehand,
                // so this is reported as "lesson not found" for the MVP.
                Some("23503") => DomainError::InvalidLessonReference,
                _ => DomainError::LessonNotFound,
            },
            _ => DomainError::LessonNotFound,
        }
    }
}

#[async_trait::async_trait]
impl LessonRepository for LessonRepositoryPg {
    // === Каталог уроков ===

    /// Fetches a lesson by ID.
    /// Returns the lesson regardless of its `is_active` flag (soft-delete aware).
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, lesson_id: Uuid) -> Result<Lesson, DomainError> {
        let row = sqlx::query_as::<_, LessonRow>(
            r#"
            SELECT lesson_id, class_id, group_id, subject_id, is_active, created_at
            FROM lessons
            WHERE lesson_id = $1
            "#,
        )
        .bind(lesson_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Saves or updates a lesson.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a lesson with the same `lesson_id` exists, it updates all mutable fields.
    /// If a lesson with the same (class + subject) or (group + subject) exists but
    /// with a different `lesson_id`, it raises a unique violation mapped to
    /// `DomainError::LessonAlreadyExists`.
    ///
    /// The `LessonTarget` is decomposed into the two nullable columns
    /// (class_id, group_id), satisfying the CHECK constraint `chk_one_entity`.
    /// `updated_at` is maintained by the `trigger_lessons_updated_at` trigger.
    async fn save(&self, lesson: Lesson) -> Result<Lesson, DomainError> {
        let (class_id, group_id) = match lesson.target {
            LessonTarget::Class(id) => (Some(id), None),
            LessonTarget::Group(id) => (None, Some(id)),
        };
        sqlx::query(
            r#"
            INSERT INTO lessons (lesson_id, class_id, group_id, subject_id, is_active)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (lesson_id) DO UPDATE SET
                class_id = EXCLUDED.class_id,
                group_id = EXCLUDED.group_id,
                subject_id = EXCLUDED.subject_id,
                is_active = EXCLUDED.is_active
            "#,
        )
        .bind(lesson.id)
        .bind(class_id)
        .bind(group_id)
        .bind(lesson.subject_id)
        .bind(lesson.is_active)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(lesson)
    }

    /// Fetches all active lessons for a specific class, sorted by subject name.
    ///
    /// Only active lessons are returned (soft-delete pattern); use `get_by_id`
    /// to fetch a deactivated lesson. Sorted by subject name for meaningful,
    /// deterministic ordering in the UI.
    ///
    /// Performance: relies on the partial index `idx_lessons_class`.
    async fn get_by_class(&self, class_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        let rows = sqlx::query_as::<_, LessonRow>(
            r#"
            SELECT l.lesson_id, l.class_id, l.group_id, l.subject_id, l.is_active, l.created_at
            FROM lessons l
            JOIN subjects s ON s.subject_id = l.subject_id
            WHERE l.class_id = $1 AND l.is_active = TRUE
            ORDER BY s.name
            "#,
        )
        .bind(class_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(LessonRow::into_domain).collect()
    }

    /// Fetches all active lessons for a specific student group, sorted by subject name.
    ///
    /// Only active lessons are returned (soft-delete pattern); use `get_by_id`
    /// to fetch a deactivated lesson.
    ///
    /// Performance: relies on the partial index `idx_lessons_group`.
    async fn get_by_group(&self, group_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        let rows = sqlx::query_as::<_, LessonRow>(
            r#"
            SELECT l.lesson_id, l.class_id, l.group_id, l.subject_id, l.is_active, l.created_at
            FROM lessons l
            JOIN subjects s ON s.subject_id = l.subject_id
            WHERE l.group_id = $1 AND l.is_active = TRUE
            ORDER BY s.name
            "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(LessonRow::into_domain).collect()
    }

    /// Fetches all active lessons taught by a specific teacher, sorted by subject name.
    ///
    /// Only active lessons are returned (soft-delete pattern). A teacher may teach
    /// the same subject to multiple classes/groups, so `lesson_id` is used as a
    /// deterministic tiebreaker.
    ///
    /// Performance: relies on `idx_lesson_teachers_teacher`.
    async fn get_by_teacher(&self, teacher_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        let rows = sqlx::query_as::<_, LessonRow>(
            r#"
            SELECT l.lesson_id, l.class_id, l.group_id, l.subject_id, l.is_active, l.created_at
            FROM lessons l
            JOIN lesson_teachers lt ON lt.lesson_id = l.lesson_id
            JOIN subjects s ON s.subject_id = l.subject_id
            WHERE lt.teacher_id = $1 AND l.is_active = TRUE
            ORDER BY s.name, l.lesson_id
            "#,
        )
        .bind(teacher_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(LessonRow::into_domain).collect()
    }

    // === Учителя урока (lesson_teachers) ===

    /// Assigns a teacher to a lesson (idempotent).
    ///
    /// `ON CONFLICT (lesson_id, teacher_id) DO NOTHING` makes repeated calls safe.
    /// If the lesson does not exist, the FK constraint raises a violation mapped
    /// to `DomainError::LessonNotFound`.
    async fn assign_teacher(&self, lesson_id: Uuid, teacher_id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO lesson_teachers (lesson_id, teacher_id)
            VALUES ($1, $2)
            ON CONFLICT (lesson_id, teacher_id) DO NOTHING
            "#,
        )
        .bind(lesson_id)
        .bind(teacher_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    /// Removes a teacher from a lesson (idempotent).
    ///
    /// Removing a non-assigned teacher is a no-op. If the lesson does not exist,
    /// returns `DomainError::LessonNotFound` (honours the trait contract, since a
    /// bare `DELETE` would not raise an error for a missing lesson).
    async fn unassign_teacher(&self, lesson_id: Uuid, teacher_id: Uuid) -> Result<(), DomainError> {
        self.ensure_lesson_exists(lesson_id).await?;
        sqlx::query(
            r#"
            DELETE FROM lesson_teachers
            WHERE lesson_id = $1 AND teacher_id = $2
            "#,
        )
        .bind(lesson_id)
        .bind(teacher_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    /// Fetches the IDs of all teachers assigned to a lesson.
    ///
    /// Returns an empty vector if the lesson has no teachers (or does not exist);
    /// use `get_by_id` to distinguish these cases if needed.
    async fn get_teacher_ids(&self, lesson_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT teacher_id
            FROM lesson_teachers
            WHERE lesson_id = $1
            ORDER BY created_at, teacher_id
            "#,
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(rows.into_iter().map(|(teacher_id,)| teacher_id).collect())
    }
}
