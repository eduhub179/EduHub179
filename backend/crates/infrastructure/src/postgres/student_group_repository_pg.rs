//! PostgreSQL implementation of `StudentGroupRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_all` / `get_groups_by_student` rely on `idx_student_groups_name`.
//! - `get_member_ids` relies on `idx_group_members_group`.
//! - `get_groups_by_student` relies on `idx_group_members_student`.
//! - `save` uses `ON CONFLICT (group_id)` for atomic upsert; the unique index
//!   `idx_student_groups_name` guards against duplicate names.
//! - `add_member` uses `ON CONFLICT (student_id, group_id) DO NOTHING` (idempotent).
use domain::entities::student_group::StudentGroup;
use domain::errors::DomainError;
use domain::repositories::student_group_repository::StudentGroupRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL (`student_groups`).
/// Kept private to isolate database schema from domain model.
/// Contains the technical `created_at` field that is not part of the domain.
#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct StudentGroupRow {
    group_id: Uuid,
    name: String,
    /// Technical field: when the group was created.
    /// Not exposed to the domain layer — kept for auditing/logging if needed.
    created_at: chrono::DateTime<chrono::Utc>,
}

impl StudentGroupRow {
    /// Converts the database row into a domain `StudentGroup` entity.
    /// Returns `Err` if the name is invalid (data corruption in DB).
    ///
    /// Note: `created_at` is intentionally ignored here.
    /// The domain model does not need this technical field for MVP.
    fn into_domain(self) -> Result<StudentGroup, DomainError> {
        StudentGroup::try_new(self.group_id, self.name)
    }
}

/// PostgreSQL-backed implementation of `StudentGroupRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct StudentGroupRepositoryPg {
    pool: PgPool,
}

impl StudentGroupRepositoryPg {
    /// Creates a new repository instance.
    /// Fail-safe: Does not validate the pool connection here;
    /// connection issues will surface on the first query.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies that a group with the given ID exists.
    /// Returns `Err(StudentGroupNotFound)` otherwise.
    ///
    /// Used by operations that must explicitly honour the "group not found"
    /// contract even when the underlying SQL would not raise an error
    /// (e.g. a bare `DELETE` on a non-existent group affects 0 rows silently).
    async fn ensure_group_exists(&self, group_id: Uuid) -> Result<(), DomainError> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT group_id FROM student_groups WHERE group_id = $1")
                .bind(group_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Self::map_db_error)?;
        match row {
            Some(_) => Ok(()),
            None => Err(DomainError::StudentGroupNotFound),
        }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    /// This is the single point of error translation, ensuring
    /// business logic never sees database-specific errors.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::StudentGroupNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation (idx_student_groups_name)
                Some("23505") => DomainError::StudentGroupAlreadyExists,
                // 23503 = foreign_key_violation (group or student does not exist).
                // The use-case layer is expected to validate students beforehand,
                // so this is reported as "group not found" for the MVP.
                Some("23503") => DomainError::StudentGroupNotFound,
                _ => DomainError::StudentGroupNotFound,
            },
            _ => DomainError::StudentGroupNotFound,
        }
    }
}

#[async_trait::async_trait]
impl StudentGroupRepository for StudentGroupRepositoryPg {
    /// Fetches a group by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, group_id: Uuid) -> Result<StudentGroup, DomainError> {
        let row = sqlx::query_as::<_, StudentGroupRow>(
            r#"
            SELECT group_id, name, created_at
            FROM student_groups
            WHERE group_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches all groups, sorted alphabetically by name.
    /// Performance: Uses the unique index `idx_student_groups_name`.
    async fn get_all(&self) -> Result<Vec<StudentGroup>, DomainError> {
        let rows = sqlx::query_as::<_, StudentGroupRow>(
            r#"
            SELECT group_id, name, created_at
            FROM student_groups
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(StudentGroupRow::into_domain).collect()
    }

    /// Saves or updates a group.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a group with the same `group_id` exists, it updates the name.
    /// If a group with the same `name` exists (but different `group_id`),
    /// it raises a unique violation, mapped to `DomainError::StudentGroupAlreadyExists`.
    ///
    /// `updated_at` is maintained by the `trigger_student_groups_updated_at` trigger.
    async fn save(&self, group: StudentGroup) -> Result<StudentGroup, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO student_groups (group_id, name)
            VALUES ($1, $2)
            ON CONFLICT (group_id) DO UPDATE SET
                name = EXCLUDED.name
            "#,
        )
        .bind(group.id)
        .bind(&group.name)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(group)
    }
    /// Adds a student to a group (idempotent).
    ///
    /// `ON CONFLICT (student_id, group_id) DO NOTHING` makes repeated calls safe.
    /// If the group does not exist, the FK constraint raises a violation,
    /// mapped to `DomainError::StudentGroupNotFound`.
    ///
    /// Performance: relies on `idx_group_members_unique`.
    async fn add_member(&self, group_id: Uuid, student_id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO group_members (student_id, group_id)
            VALUES ($1, $2)
            ON CONFLICT (student_id, group_id) DO NOTHING
            "#,
        )
        .bind(student_id)
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    /// Adds multiple students to a group in a single query (idempotent).
    ///
    /// Uses `UNNEST` for efficient bulk insertion.
    /// `ON CONFLICT (student_id, group_id) DO NOTHING` guarantees that
    /// repeated calls with the same data will not cause errors or duplicates.
    ///
    /// Performance: O(1) database round-trip regardless of the array size.
    /// Relies on the `idx_group_members_unique` index for conflict resolution.
    async fn add_members(&self, group_id: Uuid, student_ids: &[Uuid]) -> Result<(), DomainError> {
        // Fail-safe: early return for empty arrays to avoid unnecessary database hits
        if student_ids.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT INTO group_members (student_id, group_id)
            SELECT student_id, $2
            FROM unnest($1::uuid[]) AS student_id
            ON CONFLICT (student_id, group_id) DO NOTHING
            "#,
        )
        .bind(student_ids) // sqlx automatically maps `&[Uuid]` to PostgreSQL `uuid[]`
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    /// Removes a student from a group (idempotent).
    ///
    /// Removing a non-member is a no-op. If the group does not exist,
    /// returns `DomainError::StudentGroupNotFound` (honours the trait contract,
    /// since a bare `DELETE` would not raise an error for a missing group).
    async fn remove_member(&self, group_id: Uuid, student_id: Uuid) -> Result<(), DomainError> {
        self.ensure_group_exists(group_id).await?;
        sqlx::query(
            r#"
            DELETE FROM group_members
            WHERE group_id = $1 AND student_id = $2
            "#,
        )
        .bind(group_id)
        .bind(student_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(())
    }

    /// Fetches the IDs of all students in a group.
    ///
    /// Returns an empty vector if the group has no members (or does not exist);
    /// use `get_by_id` to distinguish these cases if needed.
    ///
    /// Performance: relies on `idx_group_members_group`.
    async fn get_member_ids(&self, group_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT student_id
            FROM group_members
            WHERE group_id = $1
            ORDER BY created_at, member_id
            "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(rows.into_iter().map(|(student_id,)| student_id).collect())
    }

    /// Fetches all groups a student belongs to, sorted by group name.
    ///
    /// Returns an empty vector if the student is not a member of any group.
    ///
    /// Performance: relies on `idx_group_members_student` + `idx_student_groups_name`.
    async fn get_groups_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<StudentGroup>, DomainError> {
        let rows = sqlx::query_as::<_, StudentGroupRow>(
            r#"
            SELECT sg.group_id, sg.name, sg.created_at
            FROM student_groups sg
            JOIN group_members gm ON gm.group_id = sg.group_id
            WHERE gm.student_id = $1
            ORDER BY sg.name
            "#,
        )
        .bind(student_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(StudentGroupRow::into_domain).collect()
    }
    async fn has_member(&self, group_id: Uuid, student_id: Uuid) -> Result<bool, DomainError> {
        let row: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT 1
            FROM group_members
            WHERE group_id = $1 AND student_id = $2
            "#,
        )
        .bind(group_id)
        .bind(student_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(row.is_some())
    }
}
