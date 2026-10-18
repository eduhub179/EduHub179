//! Repository trait for Telegram authorization session state.
//!
//! Dependencies: `crate::value_objects::telegram_session_state`, `crate::errors`.
//! Guarantees: All methods return `Result`. No panics.
use crate::errors::DomainError;
use crate::value_objects::telegram_session_state::TelegramSessionState;

#[async_trait::async_trait]
pub trait TelegramSessionRepository: Send + Sync {
    /// Returns the current session state for the Telegram user.
    /// Returns `Unauthenticated` if no session exists.
    async fn get_state(&self, telegram_id: i64) -> Result<TelegramSessionState, DomainError>;

    /// Sets the session state with a TTL in seconds.
    async fn set_state(
        &self,
        telegram_id: i64,
        state: TelegramSessionState,
        ttl_seconds: u64,
    ) -> Result<(), DomainError>;

    /// Deletes the session (e.g., after successful authorization).
    async fn delete(&self, telegram_id: i64) -> Result<(), DomainError>;
}
