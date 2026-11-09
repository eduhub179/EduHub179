//! Rules for a password a person chooses (first login, admin reset).
//!
//! One place for the rule, so the first-login form and the admin panel cannot drift apart.
//!
//! Dependencies: `crate::errors`.
//! Guarantees: no panics; the raw password is only checked, never stored or logged here.

use crate::errors::DomainError;

/// Allowed length in characters (not bytes: Cyrillic letters are 2 bytes in UTF-8).
pub const PASSWORD_CHARS: std::ops::RangeInclusive<usize> = 8..=128;

/// Checks a new password.
///
/// # Fail-safe поведение
/// `InvalidPasswordFormat` if it is shorter than 8 or longer than 128 characters, or
/// consists only of whitespace.
pub fn validate_new_password(raw: &str) -> Result<(), DomainError> {
    if !PASSWORD_CHARS.contains(&raw.chars().count()) || raw.trim().is_empty() {
        return Err(DomainError::InvalidPasswordFormat);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_eight_to_128_chars() {
        assert!(validate_new_password("12345678").is_ok());
        assert!(validate_new_password(&"я".repeat(128)).is_ok());
    }

    #[test]
    fn rejects_short_long_and_blank() {
        for bad in ["1234567", &"x".repeat(129), "         "] {
            assert_eq!(validate_new_password(bad), Err(DomainError::InvalidPasswordFormat));
        }
    }
}
