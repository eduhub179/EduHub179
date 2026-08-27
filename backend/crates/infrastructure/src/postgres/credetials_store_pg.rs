//! PostgreSQL implementation of the `CredentialsStore` port.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate (ports + errors),
//! internal `PasswordHasher` trait.
//!
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - The stored password hash NEVER crosses this layer boundary: it is fetched
//!   and verified (or produced and stored) entirely inside this adapter. Only
//!   raw passwords (transiently) and boolean results cross the layer boundary.
//!
//! Security notes:
//! - `verify_password` returns `Ok(false)` — not an error — when the user does
//!   not exist or has no password set. This is a deliberate fail-safe choice
//!   that prevents information leakage (an attacker cannot distinguish
//!   "user not found" from "wrong password").
//! - The hash lives only in the local stack frame of `verify_password` and is
//!   dropped immediately after verification. It is never returned to the
//!   caller, never logged, and never stored in any domain entity.
//! - `set_password` accepts a raw password and hashes it internally; the caller
//!   (use case) never sees the hash.
//!
//! Performance notes:
//! - Both methods use the primary-key index on `users.user_id` (O(log n)).
//! - `set_password` runs in a single UPDATE statement; `updated_at` is
//!   maintained by the `trigger_users_updated_at` trigger.

use domain::errors::DomainError;
use domain::ports::auth::CredentialsStore;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::password_hasher::PasswordHasher;

/// PostgreSQL-backed implementation of the `CredentialsStore` port.
///
/// Manages the `users.password_hash` column. The domain `User` entity never
/// carries password data; all hashing/verification is delegated to the
/// injected `PasswordHasher` (Argon2id in production).
pub struct CredentialsStorePg {
    pool: PgPool,
    /// Injected: hashing/verification stays an infrastructure concern.
    /// Shared via `Arc` because the same hasher is also used by
    /// `AuthCodeStorePg` (for hashing one-time codes).
    hasher: Arc<dyn PasswordHasher>,
}

impl CredentialsStorePg {
    /// Creates a new repository instance.
    ///
    /// Fail-safe: does not validate the pool connection here; connection
    /// issues will surface on the first query.
    pub fn new(pool: PgPool, hasher: Arc<dyn PasswordHasher>) -> Self {
        Self { pool, hasher }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    ///
    /// This is the single point of error translation, ensuring business logic
    /// never sees database-specific errors.
    ///
    /// Note: for `verify_password` we deliberately map `RowNotFound` to
    /// `Ok(false)` (no error) — see the security notes above. This mapping is
    /// used only by `set_password`, where a missing user must be reported.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::UserNotFound,
            sqlx::Error::Database(db_err) => {
                // 23503 = foreign_key_violation: user_id references a missing user.
                // Should not happen through the use-case layer (user is validated
                // before calling set_password), but mapped defensively.
                if db_err.code().as_deref() == Some("23503") {
                    DomainError::UserNotFound
                } else {
                    DomainError::InternalError
                }
            }
            _ => DomainError::InternalError,
        }
    }
}

#[async_trait::async_trait]
impl CredentialsStore for CredentialsStorePg {
    /// Checks a raw password against the stored hash.
    ///
    /// Returns `Ok(false)` in all of the following cases (fail-safe, no leak):
    /// - the user does not exist;
    /// - the user has no password set (`password_hash` is NULL);
    /// - the password does not match;
    /// - the stored hash is malformed (cannot be parsed by Argon2).
    ///
    /// Only `Ok(true)` indicates a successful match.
    ///
    /// Security: the stored hash is fetched into a local variable, verified,
    /// and immediately dropped. It never crosses the layer boundary.
    async fn verify_password(
        &self,
        user_id: Uuid,
        raw_password: &str,
    ) -> Result<bool, DomainError> {
        // Fetch the stored hash. We deliberately do NOT raise an error when
        // the user is missing — that would leak information about user
        // existence. `fetch_optional` returns `None` in both cases.
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT password_hash FROM users WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| DomainError::InternalError)?;

        // Missing user OR NULL hash → Ok(false): fail-safe, no information leak.
        let hash = match row.and_then(|(h,)| h) {
            Some(h) => h,
            None => return Ok(false),
        };

        // The hash lives only in this stack frame and is dropped right after
        // verification. The `PasswordHasher::verify` implementation returns
        // `Ok(false)` for a malformed stored hash (never `Err`), so we do not
        // leak whether the stored hash was corrupted.
        self.hasher.verify(raw_password, &hash)
    }

    /// Hashes the raw password and stores it (replacing any existing one).
    ///
    /// Returns `Err(DomainError::UserNotFound)` if the user does not exist.
    ///
    /// Security: the raw password is hashed inside this adapter; the hash is
    /// written to the database and immediately dropped from memory. The caller
    /// (use case) never sees the hash.
    ///
    /// Performance: single UPDATE statement on the primary key. The
    /// `updated_at` column is maintained by the `trigger_users_updated_at`
    /// trigger (defined in `0001_create_users.sql`).
    async fn set_password(&self, user_id: Uuid, raw_password: &str) -> Result<(), DomainError> {
        // Hash the password inside the adapter — the hash never leaves this
        // method's scope.
        let hash = self.hasher.hash(raw_password)?;

        let result = sqlx::query(
            "UPDATE users SET password_hash = $1, updated_at = NOW() WHERE user_id = $2",
        )
        .bind(hash)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        // `rows_affected == 0` means the user_id does not exist. We must
        // report this explicitly so the use-case layer can reject the request
        // (rather than silently succeeding).
        if result.rows_affected() == 0 {
            return Err(DomainError::UserNotFound);
        }
        Ok(())
    }
}
