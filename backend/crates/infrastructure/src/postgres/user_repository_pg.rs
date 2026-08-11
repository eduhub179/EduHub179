//! PostgreSQL implementation of `UserRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses partial indexes defined in migrations for optimal performance.

use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL.
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    user_id: Uuid,
    email: String,
    role: String, // Read as String after an explicit cast in SQL
    last_name: String,
    first_name: String,
    middle_name: Option<String>,
    is_active: bool,
    class_id: Option<Uuid>,
}

impl UserRow {
    /// Converts the database row into a domain `User` entity.
    fn into_domain(self) -> Result<User, DomainError> {
        // Parse the DB string into our Value Object
        let role = UserRole::from_str(&self.role)
            .map_err(|_| DomainError::UserNotFound)?;

        // Create the user and restore is_active from the DB
        User::try_new(
            self.user_id,
            self.email,
            role,
            self.last_name,
            self.first_name,
            self.middle_name,
            self.class_id,
        ).map(|mut user| {
            user.is_active = self.is_active;
            user
        })
    }
}

/// PostgreSQL-backed implementation of `UserRepository`.
pub struct UserRepositoryPg {
    pool: PgPool,
}

impl UserRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::UserNotFound,
            sqlx::Error::Database(db_err) => {
                if db_err.code().as_deref() == Some("23505") {
                    DomainError::EmailAlreadyExists
                } else {
                    DomainError::UserNotFound
                }
            }
            _ => DomainError::UserNotFound,
        }
    }
}

#[async_trait::async_trait]
impl UserRepository for UserRepositoryPg {
    async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                user_id, email, role::TEXT AS role, last_name, first_name,
                middle_name, is_active, class_id
            FROM users
            WHERE user_id = $1
            "#,
        )
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    async fn get_by_email(&self, email: &str) -> Result<User, DomainError> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                user_id, email, role::TEXT AS role, last_name, first_name,
                middle_name, is_active, class_id
            FROM users
            WHERE email = $1
            "#,
        )
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    async fn get_active_students_by_class(&self, class_id: Uuid) -> Result<Vec<User>, DomainError> {
        let rows = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                user_id, email, role::TEXT AS role, last_name, first_name,
                middle_name, is_active, class_id
            FROM users
            WHERE class_id = $1
              AND role = 'student'
              AND is_active = TRUE
            ORDER BY last_name, first_name
            "#,
        )
            .bind(class_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        rows.into_iter()
            .map(UserRow::into_domain)
            .collect()
    }

    async fn save(&self, user: User) -> Result<User, DomainError> {
        let role_str = user.role.to_string();

        sqlx::query(
            r#"
            INSERT INTO users (user_id, email, role, last_name, first_name, middle_name, is_active, class_id)
            VALUES ($1, $2, $3::user_role, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id) DO UPDATE SET
                email = EXCLUDED.email,
                last_name = EXCLUDED.last_name,
                first_name = EXCLUDED.first_name,
                middle_name = EXCLUDED.middle_name,
                is_active = EXCLUDED.is_active,
                class_id = EXCLUDED.class_id,
                updated_at = NOW()
            "#,
        )
            .bind(user.id)
            .bind(&user.email)
            .bind(&role_str)
            .bind(&user.last_name)
            .bind(&user.first_name)
            .bind(&user.middle_name)
            .bind(user.is_active)
            .bind(user.class_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_db_error)?;

        Ok(user)
    }
}