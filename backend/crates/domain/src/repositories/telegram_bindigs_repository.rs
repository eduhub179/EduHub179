//! Repository trait for Telegram ↔ user bindings.
//!
//! Dependencies: Only `crate::errors::DomainError` and `uuid::Uuid`.
//! Guarantees: All methods return `Result`. No panics.
//! Implementation of this trait is located in the `infrastructure` crate.
//!
//! This trait is the single point of interaction with the `telegram_bindings`
//! table. The domain layer never knows about PostgreSQL or Redis — it only
//! knows that a Telegram chat_id can be mapped to a user_id.
//!
//! The auth code itself is stored in Redis (via `AuthCodeStore`), not here.
//! This repository stores only the fact that a binding attempt is in progress
//! (status = 'pending') or completed (status = 'active').
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the Telegram binding storage.
///
/// A binding links an external Telegram chat_id (i64) to an internal user_id
/// (UUID). The lifecycle is:
/// - `store_pending_binding`: user entered email, code generated in Redis
/// - `activate_binding`: user entered code, binding becomes active
/// - `get_active_user_by_telegram_id`: bot looks up user_id for a message
#[async_trait::async_trait]
pub trait TelegramBindingRepository: Send + Sync {
    /// Returns the user_id bound to the given Telegram chat_id.
    ///
    /// Fail-safe: Returns `TelegramBindingNotFound` if no active binding
    /// exists (rather than `None`, forcing the caller to handle this case).
    async fn get_active_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError>;

    /// Stores a pending binding for the Telegram user.
    ///
    /// The auth code itself is stored in Redis (via `AuthCodeStore`), not here.
    /// This method only records that a binding attempt is in progress.
    ///
    /// Fail-safe: Returns `TelegramBindingAlreadyActive` if the user is
    /// already authorized (prevents accidental re-binding).
    async fn store_pending_binding(
        &self,
        telegram_id: i64,
        user_id: Uuid,
    ) -> Result<(), DomainError>;

    /// Activates the binding for the given Telegram chat_id.
    ///
    /// Returns the user_id of the activated binding.
    ///
    /// Fail-safe: Returns `TelegramBindingNotFound` if no pending binding
    /// exists for this telegram_id (user must call `store_pending_binding` first).
    async fn activate_binding(&self, telegram_id: i64) -> Result<Uuid, DomainError>;

    /// Returns the user_id for a pending binding.
    ///
    /// Fail-safe: Returns `TelegramBindingNotFound` if no pending binding exists
    /// (e.g., session expired or tampered with), forcing the caller to restart the flow.
    async fn get_pending_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError>;
}
