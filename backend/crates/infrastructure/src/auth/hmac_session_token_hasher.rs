//! HMAC-SHA256 implementation of `SessionTokenHasher`.
//!
//! Deterministic hashing is required for DB lookup by token hash.
//! A secret key (pepper) protects hashes if the DB is leaked.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use domain::errors::DomainError;
use domain::ports::session_security::SessionTokenHasher;
use domain::value_objects::session_token_hash::SessionTokenHash;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 session token hasher.
pub struct HmacSessionTokenHasher {
    key: Vec<u8>,
}

impl HmacSessionTokenHasher {
    /// Creates a new hasher with a non-empty secret key.
    pub fn try_new(secret: &str) -> Result<Self, DomainError> {
        let normalized = secret.trim();
        if normalized.is_empty() {
            return Err(DomainError::InternalError);
        }

        Ok(Self {
            key: normalized.as_bytes().to_vec(),
        })
    }

    fn hash_raw(&self, raw_token: &str) -> Result<String, DomainError> {
        let token = raw_token.trim();
        if token.is_empty() {
            return Err(DomainError::InvalidCredentials);
        }

        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| DomainError::InternalError)?;
        mac.update(token.as_bytes());
        let bytes = mac.finalize().into_bytes();
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
}

impl SessionTokenHasher for HmacSessionTokenHasher {
    fn hash_session_token(&self, raw_token: &str) -> Result<SessionTokenHash, DomainError> {
        let encoded = self.hash_raw(raw_token)?;
        SessionTokenHash::try_new(&encoded)
    }

    fn verify_session_token(
        &self,
        raw_token: &str,
        stored_hash: &SessionTokenHash,
    ) -> Result<bool, DomainError> {
        let recomputed = self.hash_raw(raw_token)?;
        let left = recomputed.as_bytes();
        let right = stored_hash.as_str().as_bytes();

        if left.len() != right.len() {
            return Ok(false);
        }

        Ok(left.ct_eq(right).into())
    }
}
