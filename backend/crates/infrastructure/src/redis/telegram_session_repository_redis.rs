//! Redis implementation of `TelegramSessionRepository`.
//!
//! Dependencies: `redis` crate, `domain` crate.
//! Guarantees: All methods return `Result`. No panics.
//!
//! Key format: `tg_session:{telegram_id}`
//! Value: state string (e.g., "awaiting_email")
//! TTL: set via `SET EX` command.
use domain::errors::DomainError;
use domain::repositories::telegram_session_repository::TelegramSessionRepository;
use domain::value_objects::telegram_session_state::TelegramSessionState;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::str::FromStr;
use std::sync::Arc;

pub struct TelegramSessionRepositoryRedis {
    conn: ConnectionManager,
}

impl TelegramSessionRepositoryRedis {
    /// Creates a new repository instance.
    /// Fail-safe: connection errors return `InternalError`.
    pub async fn new(url: &str) -> Result<Self, DomainError> {
        let client = redis::Client::open(url).map_err(|_| DomainError::InternalError)?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(Self { conn })
    }

    fn key(telegram_id: i64) -> String {
        format!("tg_session:{}", telegram_id)
    }
}

#[async_trait::async_trait]
impl TelegramSessionRepository for TelegramSessionRepositoryRedis {
    async fn get_state(&self, telegram_id: i64) -> Result<TelegramSessionState, DomainError> {
        let mut conn = self.conn.clone();
        let state: Option<String> = conn
            .get(Self::key(telegram_id))
            .await
            .map_err(|_| DomainError::InternalError)?;

        match state {
            Some(s) => TelegramSessionState::from_str(&s).map_err(|_| DomainError::InternalError),
            None => Ok(TelegramSessionState::Unauthenticated),
        }
    }

    async fn set_state(
        &self,
        telegram_id: i64,
        state: TelegramSessionState,
        ttl_seconds: u64,
    ) -> Result<(), DomainError> {
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(Self::key(telegram_id), state.to_string(), ttl_seconds)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(())
    }

    async fn delete(&self, telegram_id: i64) -> Result<(), DomainError> {
        let mut conn = self.conn.clone();
        conn.del::<_, ()>(Self::key(telegram_id))
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(())
    }
}
