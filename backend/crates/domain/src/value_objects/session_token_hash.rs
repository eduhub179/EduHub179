//! Value Object for hashed session tokens.
//!
//! Represents the persisted hash of an opaque cookie session token.

use crate::errors::DomainError;

const MAX_HASH_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionTokenHash(String);

impl SessionTokenHash {
    /// Validates and normalizes a stored hash value.
    pub fn try_new(raw: &str) -> Result<Self, DomainError> {
        let normalized = raw.trim();
        if normalized.is_empty() || normalized.len() > MAX_HASH_LEN {
            return Err(DomainError::InvalidCredentials);
        }
        Ok(Self(normalized.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionTokenHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
