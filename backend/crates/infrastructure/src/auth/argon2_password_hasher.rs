//! Argon2id implementation of the `PasswordHasher` trait.
//!
//! Dependencies: `argon2` crate, `domain` crate (errors).
//! Guarantees:
//! - All methods return `Result`. No panics.
//! - Uses Argon2id (OWASP-recommended, memory-hard, resistant to GPU attacks).
//! - Salt is generated automatically (`SaltString::generate`).
//! - Parameters are Argon2 defaults (suitable for MVP).
//!
//! SECURITY NOTE: The hash lives only in the local stack frame of the caller
//! (`CredentialsStorePg` or `AuthCodeStorePg`) and is dropped immediately.
//! It never crosses the layer boundary.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{
    Argon2, PasswordHasher as Argon2HasherTrait, PasswordVerifier as Argon2VerifierTrait,
};
use domain::errors::DomainError;

use super::password_hasher::PasswordHasher;

/// Argon2id password hasher.
///
/// Stateless: no configuration needed (defaults are OWASP-recommended).
pub struct Argon2PasswordHasher;

impl PasswordHasher for Argon2PasswordHasher {
    fn hash(&self, raw_password: &str) -> Result<String, DomainError> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(raw_password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|_| DomainError::InternalError)
    }

    fn verify(&self, raw_password: &str, hashed: &str) -> Result<bool, DomainError> {
        // A malformed stored hash is treated as a verification failure,
        // never as an internal error (fail-safe, no information leak).
        let parsed = match PasswordHash::new(hashed) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };
        Ok(Argon2::default()
            .verify_password(raw_password.as_bytes(), &parsed)
            .is_ok())
    }
}
