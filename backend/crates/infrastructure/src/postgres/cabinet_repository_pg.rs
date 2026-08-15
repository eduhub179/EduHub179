//! PostgreSQL implementation of `CabinetRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//!
//! Performance notes:
//! - `get_by_id` / `get_by_number` use the primary key / unique index (O(log n)).
//! - `get_by_floor` relies on `idx_cabinets_floor`.
//! - `save` uses `ON CONFLICT (cabinet_id)` for atomic upsert.
//!
//! IMPORTANT: the `floor` column is `GENERATED ALWAYS AS (number / 100) STORED` —
//! it must NOT appear in INSERT/UPDATE column lists; Postgres computes it itself.

use domain::entities::cabinet::Cabinet;
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
/// `floor` is deliberately not selected: it is derived from `number` in the domain.
#[derive(Debug, sqlx::FromRow)]
struct CabinetRow {
    cabinet_id: Uuid,
    number: i32,
    description: Option<String>,
    capacity: Option<i32>,
}

impl CabinetRow {
    /// Converts the database row into a domain `Cabinet` entity.
    /// Returns `Err` if the row violates domain invariants (data corruption in DB).
    fn into_domain(self) -> Result<Cabinet, DomainError> {
        Cabinet::try_new(
            self.cabinet_id,
            self.number,
            self.description,
            self.capacity,
        )
    }
}

/// PostgreSQL-backed implementation of `CabinetRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct CabinetRepositoryPg {
    pool: PgPool,
}

impl CabinetRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::CabinetNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23505 = unique_violation (cabinets_number_key)
                Some("23505") => DomainError::CabinetAlreadyExists,
                // 23514 = check_violation (number/capacity CHECK constraints).
                // Unreachable through the domain API (validated in `try_new`);
                // mapped to the closest domain error for fail-safe.
                Some("23514") => DomainError::InvalidCabinetNumber,
                _ => DomainError::CabinetNotFound,
            },
            _ => DomainError::CabinetNotFound,
        }
    }
}

#[async_trait::async_trait]
impl CabinetRepository for CabinetRepositoryPg {
    /// Fetches a cabinet by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, cabinet_id: Uuid) -> Result<Cabinet, DomainError> {
        let row = sqlx::query_as::<_, CabinetRow>(
            r#"
            SELECT cabinet_id, number, description, capacity
            FROM cabinets
            WHERE cabinet_id = $1
            "#,
        )
        .bind(cabinet_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches a cabinet by its unique number.
    /// Performance: Uses the unique index on `number` (O(log n)).
    async fn get_by_number(&self, number: i32) -> Result<Cabinet, DomainError> {
        let row = sqlx::query_as::<_, CabinetRow>(
            r#"
            SELECT cabinet_id, number, description, capacity
            FROM cabinets
            WHERE number = $1
            "#,
        )
        .bind(number)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        row.into_domain()
    }

    /// Fetches all cabinets, sorted by number.
    async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError> {
        let rows = sqlx::query_as::<_, CabinetRow>(
            r#"
            SELECT cabinet_id, number, description, capacity
            FROM cabinets
            ORDER BY number
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(CabinetRow::into_domain).collect()
    }

    /// Fetches all cabinets on a floor, sorted by number.
    /// Performance: relies on `idx_cabinets_floor`.
    async fn get_by_floor(&self, floor: i32) -> Result<Vec<Cabinet>, DomainError> {
        let rows = sqlx::query_as::<_, CabinetRow>(
            r#"
            SELECT cabinet_id, number, description, capacity
            FROM cabinets
            WHERE floor = $1
            ORDER BY number
            "#,
        )
        .bind(floor)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        rows.into_iter().map(CabinetRow::into_domain).collect()
    }

    /// Saves or updates a cabinet (atomic upsert on `cabinet_id`).
    ///
    /// `floor` is omitted from both INSERT and UPDATE: it is a
    /// `GENERATED ALWAYS` column computed from `number` by PostgreSQL.
    async fn save(&self, cabinet: Cabinet) -> Result<Cabinet, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO cabinets (cabinet_id, number, description, capacity)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (cabinet_id) DO UPDATE SET
                number = EXCLUDED.number,
                description = EXCLUDED.description,
                capacity = EXCLUDED.capacity
            "#,
        )
        .bind(cabinet.id)
        .bind(cabinet.number)
        .bind(cabinet.description.as_deref())
        .bind(cabinet.capacity)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;
        Ok(cabinet)
    }
}
