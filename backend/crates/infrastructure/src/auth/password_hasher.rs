//! Internal password hashing interface.
//!
//! This trait lives in the infrastructure layer because password hashes
//! never leave this layer. It is consumed only by `CredentialsStorePg`
//! and `AuthCodeStorePg`.

use domain::errors::DomainError;

/// Hashes raw passwords and verifies them against stored hashes.
///
/// Synchronous by design: hashing is CPU-bound. Implementations should use
/// a memory-hard algorithm (e.g. Argon2id).
pub trait PasswordHasher: Send + Sync {
    /// Produces a hash of the raw password (includes algorithm params + salt).
    /// Returns `Err(DomainError::InternalError)` if hashing fails unexpectedly.
    fn hash(&self, raw_password: &str) -> Result<String, DomainError>;

    /// Checks whether `raw_password` matches the stored `hashed` value.
    /// Returns `Ok(false)` for a mismatch OR a malformed stored hash
    /// (never leaks whether the stored hash was corrupted).
    fn verify(&self, raw_password: &str, hashed: &str) -> Result<bool, DomainError>;
}
