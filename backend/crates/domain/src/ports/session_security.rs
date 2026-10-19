//! Session-security ports for cookie-based authentication.
//!
//! Implemented in infrastructure (e.g. HMAC/SHA-256 hasher).

use crate::errors::DomainError;
use crate::value_objects::session_token_hash::SessionTokenHash;

/// Hashes and verifies raw session tokens used in HttpOnly cookies.
///
/// SECURITY: raw token must never be persisted; only the hash is stored.
pub trait SessionTokenHasher: Send + Sync {
    /// Hashes a raw session token for persistence.
    fn hash_session_token(&self, raw_token: &str) -> Result<SessionTokenHash, DomainError>;

    /// Verifies a raw token against stored hash.
    fn verify_session_token(
        &self,
        raw_token: &str,
        stored_hash: &SessionTokenHash,
    ) -> Result<bool, DomainError>;
}

/// Generates opaque random tokens for session cookies.
pub trait SessionTokenGenerator: Send + Sync {
    /// Returns a cryptographically-random token.
    fn generate_session_token(&self) -> String;
}
