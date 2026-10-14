//! PostgreSQL implementation of `TelegramBindingRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//!
//! Performance notes:
//! - `get_active_user_by_telegram_id` relies on partial index
//!   `idx_telegram_bindings_active` (telegram_id WHERE status = 'active').
//! - `store_pending_binding` uses `ON CONFLICT (telegram_id)` for atomic upsert.
//! - `activate_binding` updates status in a single statement.
use domain::errors::DomainError;
use domain::repositories::telegram_bindigs_repository::TelegramBindingRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed implementation of `TelegramBindingRepository`.
pub struct TelegramBindingRepositoryPg {
    pool: PgPool,
}

impl TelegramBindingRepositoryPg {
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
            sqlx::Error::RowNotFound => DomainError::TelegramBindingNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23503 = foreign_key_violation (user_id references missing user)
                Some("23503") => DomainError::UserNotFound,
                // 23505 = unique_violation (should not happen with PRIMARY KEY)
                Some("23505") => DomainError::TelegramBindingAlreadyActive,
                _ => DomainError::TelegramBindingNotFound,
            },
            _ => DomainError::TelegramBindingNotFound,
        }
    }
}

#[async_trait::async_trait]
impl TelegramBindingRepository for TelegramBindingRepositoryPg {
    /// Fetches the user_id bound to the given Telegram chat_id.
    /// Performance: Uses partial index `idx_telegram_bindings_active`.
    async fn get_active_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT user_id FROM telegram_bindings
            WHERE telegram_id = $1 AND status = 'active'
            "#,
        )
        .bind(telegram_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        match row {
            Some((user_id,)) => Ok(user_id),
            None => Err(DomainError::TelegramBindingNotFound),
        }
    }

    /// Stores a pending binding for the Telegram user.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a binding with the same `telegram_id` already exists and is active,
    /// returns `TelegramBindingAlreadyActive`.
    async fn store_pending_binding(
        &self,
        telegram_id: i64,
        user_id: Uuid,
    ) -> Result<(), DomainError> {
        // Check if already active
        let existing: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT status FROM telegram_bindings
            WHERE telegram_id = $1
            "#,
        )
        .bind(telegram_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        if let Some((status,)) = existing {
            if status == "active" {
                return Err(DomainError::TelegramBindingAlreadyActive);
            }
        }

        // Insert or update pending binding
        sqlx::query(
            r#"
            INSERT INTO telegram_bindings (telegram_id, user_id, status)
            VALUES ($1, $2, 'pending')
            ON CONFLICT (telegram_id) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                status = 'pending',
                updated_at = NOW()
            "#,
        )
        .bind(telegram_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    async fn get_pending_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT user_id FROM telegram_bindings
            WHERE telegram_id = $1 AND status = 'pending'
            "#,
        )
        .bind(telegram_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        match row {
            Some((user_id,)) => Ok(user_id),
            None => Err(DomainError::TelegramBindingNotFound),
        }
    }
    /// Activates the binding for the given Telegram chat_id.
    ///
    /// Returns the user_id of the activated binding.
    /// Fail-safe: Returns `TelegramBindingNotFound` if no pending binding
    /// exists for this telegram_id.
    async fn activate_binding(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            UPDATE telegram_bindings
            SET status = 'active', updated_at = NOW()
            WHERE telegram_id = $1 AND status = 'pending'
            RETURNING user_id
            "#,
        )
        .bind(telegram_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        match row {
            Some((user_id,)) => Ok(user_id),
            None => Err(DomainError::TelegramBindingNotFound),
        }
    }
}
