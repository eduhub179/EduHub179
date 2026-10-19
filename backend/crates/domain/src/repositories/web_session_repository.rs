//! Repository trait for secure web-session persistence.
//!
//! Dependencies: `crate::errors::DomainError`, `crate::value_objects::role::UserRole`,
//! `chrono`, and `uuid`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::web_session::{NewWebSession, WebSession};
use crate::errors::DomainError;
use crate::value_objects::session_token_hash::SessionTokenHash;
use uuid::Uuid;

/// Interface for interacting with web-session storage.
#[async_trait::async_trait]
pub trait WebSessionRepository: Send + Sync {
    /// Creates a new session row.
    async fn create(&self, new_session: NewWebSession) -> Result<WebSession, DomainError>;

    /// Returns an active, non-expired session by token hash.
    ///
    /// Also resolves the user's current role from the `users` table for
    /// authorization checks.
    async fn get_active_by_token_hash(
        &self,
        token_hash: &SessionTokenHash,
    ) -> Result<WebSession, DomainError>;

    /// Updates session activity metadata on authenticated request.
    async fn touch_last_seen(
        &self,
        session_id: Uuid,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), DomainError>;

    /// Revokes one session.
    async fn revoke(&self, session_id: Uuid, reason: Option<String>) -> Result<(), DomainError>;

    /// Revokes all active sessions for one user.
    async fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        reason: Option<String>,
    ) -> Result<u64, DomainError>;

    /// Deletes expired or revoked sessions that are no longer needed.
    async fn purge_expired(&self) -> Result<u64, DomainError>;
}
