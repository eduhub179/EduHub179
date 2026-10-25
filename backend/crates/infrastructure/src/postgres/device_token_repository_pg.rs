//! PostgreSQL implementation of `DeviceTokenRepository`.

use chrono::{DateTime, NaiveDateTime, Utc};
use domain::entities::device_token::DeviceToken;
use domain::errors::DomainError;
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct DeviceTokenRow {
    device_token_id: Uuid,
    user_id: Uuid,
    platform: String,
    token: String,
    is_active: bool,
    // TIMESTAMPTZ columns must be read as DateTime<Utc>: decoding them into
    // NaiveDateTime fails at runtime (sqlx type mismatch) on every read.
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl DeviceTokenRow {
    fn into_domain(self) -> Result<DeviceToken, DomainError> {
        let platform = NotificationPlatform::from_str(&self.platform)
            .map_err(|_| DomainError::InvalidDeviceTokenFormat)?;

        DeviceToken::try_new(
            self.device_token_id,
            self.user_id,
            platform,
            self.token,
            self.created_at.naive_utc(),
            self.updated_at.naive_utc(),
        )
        .map(|mut token| {
            token.is_active = self.is_active;
            token
        })
    }
}

/// PostgreSQL-backed implementation of `DeviceTokenRepository`.
pub struct DeviceTokenRepositoryPg {
    pool: PgPool,
}

impl DeviceTokenRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Unknown failures (decode errors, lost connection) are `InternalError`: reporting
    /// them as "user not found" hid a type-mismatch bug that broke every read.
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => DomainError::UserNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => DomainError::InvalidDeviceTokenFormat,
                // 23503: user_id references a missing user.
                Some("23503") => DomainError::UserNotFound,
                _ => DomainError::InternalError,
            },
            _ => DomainError::InternalError,
        }
    }
}

#[async_trait::async_trait]
impl DeviceTokenRepository for DeviceTokenRepositoryPg {
    async fn get_by_id(&self, device_token_id: Uuid) -> Result<DeviceToken, DomainError> {
        let row = sqlx::query_as::<_, DeviceTokenRow>(
            r#"
            SELECT
                device_token_id,
                user_id,
                platform::TEXT AS platform,
                token,
                is_active,
                last_used_at,
                created_at,
                updated_at
            FROM device_tokens
            WHERE device_token_id = $1
            "#,
        )
        .bind(device_token_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    async fn get_by_user(&self, user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
        let rows = sqlx::query_as::<_, DeviceTokenRow>(
            r#"
            SELECT
                device_token_id,
                user_id,
                platform::TEXT AS platform,
                token,
                is_active,
                last_used_at,
                created_at,
                updated_at
            FROM device_tokens
            WHERE user_id = $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(DeviceTokenRow::into_domain).collect()
    }

    async fn get_active_by_user(&self, user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
        let rows = sqlx::query_as::<_, DeviceTokenRow>(
            r#"
            SELECT
                device_token_id,
                user_id,
                platform::TEXT AS platform,
                token,
                is_active,
                last_used_at,
                created_at,
                updated_at
            FROM device_tokens
            WHERE user_id = $1 AND is_active = TRUE
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(DeviceTokenRow::into_domain).collect()
    }

    async fn get_active_by_user_and_platform(
        &self,
        user_id: Uuid,
        platform: NotificationPlatform,
    ) -> Result<Vec<DeviceToken>, DomainError> {
        let rows = sqlx::query_as::<_, DeviceTokenRow>(
            r#"
            SELECT
                device_token_id,
                user_id,
                platform::TEXT AS platform,
                token,
                is_active,
                last_used_at,
                created_at,
                updated_at
            FROM device_tokens
            WHERE user_id = $1 AND platform = $2::notification_platform AND is_active = TRUE
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .bind(platform.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(DeviceTokenRow::into_domain).collect()
    }

    async fn save(&self, device_token: DeviceToken) -> Result<DeviceToken, DomainError> {
        let platform = device_token.platform.to_string();

        sqlx::query(
            r#"
            INSERT INTO device_tokens (
                device_token_id,
                user_id,
                platform,
                token,
                is_active,
                last_used_at,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3::notification_platform, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, platform, token) DO UPDATE SET
                is_active = EXCLUDED.is_active,
                last_used_at = EXCLUDED.last_used_at,
                updated_at = NOW()
            "#,
        )
        .bind(device_token.id)
        .bind(device_token.user_id)
        .bind(&platform)
        .bind(&device_token.token)
        .bind(device_token.is_active)
        .bind(None::<NaiveDateTime>)
        .bind(device_token.created_at)
        .bind(device_token.updated_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(device_token)
    }

    async fn deactivate(&self, device_token_id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE device_tokens
            SET is_active = FALSE,
                updated_at = NOW()
            WHERE device_token_id = $1
            "#,
        )
        .bind(device_token_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    async fn deactivate_token_for_other_users(
        &self,
        platform: NotificationPlatform,
        token: &str,
        keep_user_id: Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE device_tokens
            SET is_active = FALSE,
                updated_at = NOW()
            WHERE platform = $1::notification_platform
              AND token = $2
              AND user_id <> $3
              AND is_active = TRUE
            "#,
        )
        .bind(platform.to_string())
        .bind(token)
        .bind(keep_user_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }

    async fn deactivate_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE device_tokens
            SET is_active = FALSE,
                updated_at = NOW()
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(())
    }
}
