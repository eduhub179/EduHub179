//! PostgreSQL implementation of `ClassRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses partial indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_active_by_year` relies on partial index
//!   `idx_classes_graduation_year` (graduation_year) WHERE is_active=TRUE.
//! - `save` uses `ON CONFLICT (class_id)` for atomic upsert.
use domain::entities::class::Class;
use domain::value_objects::class_letter::ClassLetter;
use domain::errors::DomainError;
use domain::repositories::class_repository::ClassRepository;
use sqlx::{PgPool, Type};
use uuid::Uuid;

/// PostgreSQL ENUM wrapper for `ClassLetter`.
/// Implements `sqlx::Type` for automatic decoding from `class_letter` ENUM.
/// This type is internal to infrastructure and never leaks into the domain layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[sqlx(type_name = "class_letter")]
pub enum DbClassLetter {
    #[sqlx(rename = "б")]
    B,
    #[sqlx(rename = "в")]
    V,
    #[sqlx(rename = "и")]
    I,
}

impl From<DbClassLetter> for ClassLetter {
    fn from(db_letter: DbClassLetter) -> Self {
        match db_letter {
            DbClassLetter::B => ClassLetter::B,
            DbClassLetter::V => ClassLetter::V,
            DbClassLetter::I => ClassLetter::I,
        }
    }
}

impl From<ClassLetter> for DbClassLetter {
    fn from(letter: ClassLetter) -> Self {
        match letter {
            ClassLetter::B => DbClassLetter::B,
            ClassLetter::V => DbClassLetter::V,
            ClassLetter::I => DbClassLetter::I,
        }
    }
}

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
/// If DB schema changes, only this file needs to be updated.
#[derive(Debug, sqlx::FromRow)]
struct ClassRow {
    class_id: Uuid,
    graduation_year: i32,
    letter: DbClassLetter,
    is_active: bool,
}

impl ClassRow {
    /// Converts the database row into a domain `Class` entity.
    /// Returns `Err` if the letter string is invalid (data corruption in DB).
    fn into_domain(self) -> Result<Class, DomainError> {
        Class::try_new(
            self.class_id,
            self.graduation_year,
            self.letter.into(),
            self.is_active,
        )
    }
}

/// PostgreSQL-backed implementation of `ClassRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct ClassRepositoryPg {
    pool: PgPool,
}

impl ClassRepositoryPg {
    /// Creates a new repository instance.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    /// This is the single point of error translation, ensuring
    /// business logic never sees database-specific errors.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::ClassNotFound,
            sqlx::Error::Database(db_err) => {
                match db_err.code().as_deref() {
                    // 23505 = unique_violation (idx_classes_year_letter)
                    Some("23505") => DomainError::ClassAlreadyExists,
                    // 42P01 = undefined_table (таблица не существует)
                    Some("42P01") => {
                        eprintln!("DATABASE ERROR: Table 'classes' does not exist. Did you run migrations?");
                        DomainError::ClassNotFound
                    }
                    // 42704 = undefined_object (ENUM тип не существует)
                    Some("42704") => {
                        eprintln!("DATABASE ERROR: Type 'class_letter' does not exist. Did you run migrations?");
                        DomainError::ClassNotFound
                    }
                    // 23503 = foreign_key_violation
                    Some("23503") => {
                        eprintln!("DATABASE ERROR: Foreign key violation");
                        DomainError::ClassNotFound
                    }
                    _ => {
                        eprintln!("DATABASE ERROR: {} (code: {:?})", db_err.message(), db_err.code());
                        DomainError::ClassNotFound
                    }
                }
            }
            _ => {
                eprintln!("DATABASE ERROR: {:?}", err);
                DomainError::ClassNotFound
            }
        }
    }
}

#[async_trait::async_trait]
impl ClassRepository for ClassRepositoryPg {
    /// Fetches a class by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, class_id: Uuid) -> Result<Class, DomainError> {
        let row = sqlx::query_as::<_, ClassRow>(
            r#"
            SELECT class_id, graduation_year, letter, is_active
            FROM classes
            WHERE class_id = $1
            "#,
        )
            .bind(class_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    /// Fetches all active classes for a specific graduation year, sorted by letter.
    ///
    /// Performance: This query is optimized by the partial index:
    /// `CREATE INDEX idx_classes_graduation_year ON classes (graduation_year) WHERE is_active = TRUE;`
    async fn get_active_by_year(&self, graduation_year: i32) -> Result<Vec<Class>, DomainError> {
        let rows = sqlx::query_as::<_, ClassRow>(
            r#"
            SELECT class_id, graduation_year, letter, is_active
            FROM classes
            WHERE graduation_year = $1 AND is_active = TRUE
            ORDER BY letter
            "#,
        )
            .bind(graduation_year)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        rows.into_iter()
            .map(ClassRow::into_domain)
            .collect()
    }

    /// Saves or updates a class.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    async fn save(&self, class: Class) -> Result<Class, DomainError> {
        let db_letter = DbClassLetter::from(class.letter);

        sqlx::query(
            r#"
            INSERT INTO classes (class_id, graduation_year, letter, is_active)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (class_id) DO UPDATE SET
                graduation_year = EXCLUDED.graduation_year,
                letter = EXCLUDED.letter,
                is_active = EXCLUDED.is_active,
                updated_at = NOW()
            "#,
        )
            .bind(class.id)
            .bind(class.graduation_year)
            .bind(db_letter)
            .bind(class.is_active)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        Ok(class)
    }
}