//! PostgreSQL implementation of `WebSessionRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.

use chrono::{DateTime, Utc};
use domain::entities::web_session::{NewWebSession, WebSession};
use domain::errors::DomainError;
use domain::repositories::web_session_repository::WebSessionRepository;
use domain::value_objects::csrf_token_hash::CsrfTokenHash;
use domain::value_objects::role::UserRole;
use domain::value_objects::session_token_hash::SessionTokenHash;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct WebSessionRow {
    session_id: Uuid,
    user_id: Uuid,
    role: String,
    session_token_hash: String,
    csrf_token_hash: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    is_persistent: bool,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl WebSessionRow {
    fn into_domain(self) -> Result<WebSession, DomainError> {
        let role = UserRole::from_str(&self.role).map_err(|_| DomainError::InternalError)?;

        Ok(WebSession {
            session_id: self.session_id,
            user_id: self.user_id,
            role,
            session_token_hash: SessionTokenHash::try_new(&self.session_token_hash)
                .map_err(|_| DomainError::InternalError)?,
            csrf_token_hash: self
                .csrf_token_hash
                .as_deref()
                .map(CsrfTokenHash::try_new)
                .transpose()
                .map_err(|_| DomainError::InternalError)?,
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            is_persistent: self.is_persistent,
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            expires_at: self.expires_at,
        })
    }
}

/// PostgreSQL-backed implementation of `WebSessionRepository`.
pub struct WebSessionRepositoryPg {
    pool: PgPool,
}

impl WebSessionRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::InvalidCredentials,
            sqlx::Error::Database(db_err) => {
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
impl WebSessionRepository for WebSessionRepositoryPg {
    async fn create(&self, new_session: NewWebSession) -> Result<WebSession, DomainError> {
        let row = sqlx::query_as::<_, WebSessionRow>(
            r#"
            INSERT INTO web_sessions (
                user_id,
                session_token_hash,
                csrf_token_hash,
                ip_address,
                user_agent,
                is_persistent,
                expires_at
            )
            VALUES ($1, $2, $3, $4::inet, $5, $6, $7)
            RETURNING
                session_id,
                user_id,
                (SELECT role::TEXT FROM users WHERE users.user_id = web_sessions.user_id) AS role,
                session_token_hash,
                csrf_token_hash,
                ip_address::TEXT AS ip_address,
                user_agent,
                is_persistent,
                created_at,
                last_seen_at,
                expires_at
            "#,
        )
        .bind(new_session.user_id)
        .bind(new_session.session_token_hash.as_str())
        .bind(new_session.csrf_token_hash.as_ref().map(|value| value.as_str()))
        .bind(new_session.ip_address)
        .bind(new_session.user_agent)
        .bind(new_session.is_persistent)
        .bind(new_session.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    async fn get_active_by_token_hash(
        &self,
        token_hash: &SessionTokenHash,
    ) -> Result<WebSession, DomainError> {
        let row = sqlx::query_as::<_, WebSessionRow>(
            r#"
            SELECT
                s.session_id,
                s.user_id,
                u.role::TEXT AS role,
                s.session_token_hash,
                s.csrf_token_hash,
                s.ip_address::TEXT AS ip_address,
                s.user_agent,
                s.is_persistent,
                s.created_at,
                s.last_seen_at,
                s.expires_at
            FROM web_sessions s
            JOIN users u ON u.user_id = s.user_id
            WHERE s.session_token_hash = $1
              AND s.revoked_at IS NULL
              AND s.expires_at > NOW()
              AND u.is_active = TRUE
            "#,
        )
        .bind(token_hash.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    async fn touch_last_seen(
        &self,
        session_id: Uuid,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE web_sessions
            SET
                last_seen_at = NOW(),
                ip_address = COALESCE($2::inet, ip_address),
                user_agent = COALESCE($3, user_agent),
                updated_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .bind(ip_address)
        .bind(user_agent)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    async fn revoke(&self, session_id: Uuid, reason: Option<String>) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE web_sessions
            SET
                revoked_at = NOW(),
                revoke_reason = COALESCE($2, revoke_reason),
                updated_at = NOW()
            WHERE session_id = $1
              AND revoked_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    async fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        reason: Option<String>,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE web_sessions
            SET
                revoked_at = NOW(),
                revoke_reason = COALESCE($2, revoke_reason),
                updated_at = NOW()
            WHERE user_id = $1
              AND revoked_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(result.rows_affected())
    }

    async fn purge_expired(&self) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM web_sessions
            WHERE expires_at <= NOW()
               OR (revoked_at IS NOT NULL AND revoked_at <= NOW() - INTERVAL '30 days')
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(result.rows_affected())
    }
}
