//! Value Object for user login.
//!
//! Invariants:
//! - A login is the local part of the school email address.
//!   Example: login `s27b_ivanov` <=> email `s27b_ivanov@179.ru`.
//! - Only lowercase ASCII letters, digits, and the separators `_`, `.`, `-`
//!   are allowed. Length is 1..=100 characters.
//! - The organization email domain is injected at startup via
//!   [`crate::settings::init`] and read from [`crate::settings::get`].
//!   The domain layer never reads the environment directly.
//!
//! Dependencies: `crate::errors::DomainError`, `crate::settings`.
//! Guarantees: An instance can only be created via `try_new` / `from_email` /
//! `from_identifier`, which validate the invariants. This prevents invalid
//! logins from reaching the repository.
use crate::errors::DomainError;

/// Maximum number of characters in a login (matches the DB VARCHAR limit).
const MAX_LOGIN_CHARS: usize = 100;

/// A user login: the local part of the school email address.
///
/// Examples: `s27b_ivanov`, `t_ivanova`.
/// The full school email is derived via [`Login::email`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Login(String);

impl Login {
    /// Creates a login from a raw string, validating the format.
    ///
    /// The input is trimmed and lowercased before validation, so logins are
    /// case-insensitive on the way in and always stored in lowercase.
    ///
    /// Returns `Err(DomainError::InvalidLoginFormat)` if the login is empty,
    /// too long, or contains forbidden characters.
    pub fn try_new(raw: &str) -> Result<Self, DomainError> {
        let normalized = raw.trim().to_lowercase();
        if !Login::is_valid_login(&normalized) {
            return Err(DomainError::InvalidLoginFormat);
        }
        Ok(Self(normalized))
    }

    /// Parses a login from a full school email address.
    ///
    /// The email must end with the organization domain (read from
    /// [`crate::settings::get`]); the local part becomes the login. The
    /// comparison is case-insensitive.
    ///
    /// Returns:
    /// - `Err(DomainError::InvalidEmailFormat)` if the email does not belong to
    ///   the organization domain (or is empty).
    /// - `Err(DomainError::InvalidLoginFormat)` if the local part is invalid.
    pub fn from_email(email: &str) -> Result<Self, DomainError> {
        let normalized = email.trim().to_lowercase();
        let org_domain = crate::settings::get().org_email_domain.as_str();
        let local = normalized
            .strip_suffix(org_domain)
            .ok_or(DomainError::InvalidEmailFormat)?;
        Self::try_new(local)
    }

    /// Normalizes an arbitrary identifier (login or email) into a login.
    ///
    /// This is the entry point used during authentication, where the user may
    /// type either their login or their full school email.
    /// - contains `@` -> treated as an email ([`Login::from_email`]).
    /// - otherwise -> treated as a login ([`Login::try_new`]).
    pub fn from_identifier(raw: &str) -> Result<Self, DomainError> {
        if raw.contains('@') {
            Self::from_email(raw)
        } else {
            Self::try_new(raw)
        }
    }

    /// Returns the full school email address for this login.
    ///
    /// Example: `s27b_ivanov` -> `s27b_ivanov@179.ru`.
    ///
    /// Reads the organization domain from [`crate::settings::get`].
    pub fn email(&self) -> String {
        format!("{}{}", self.0, crate::settings::get().org_email_domain)
    }

    /// Returns the login as a string slice (for persistence / serialization).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates the login format.
    ///
    /// A valid login is non-empty, at most [`MAX_LOGIN_CHARS`] characters, and
    /// consists only of lowercase ASCII letters, digits, and `_` / `.` / `-`.
    fn is_valid_login(login: &str) -> bool {
        let count = login.chars().count();
        if count == 0 || count > MAX_LOGIN_CHARS {
            return false;
        }
        login
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
    }
}

impl std::fmt::Display for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain login`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Performs the mandatory settings initialization step for tests.
    /// Idempotent: the first call wins, so parallel tests are safe.
    fn init_settings() {
        crate::settings::init(crate::settings::Settings::mock());
    }

    #[test]
    fn try_new_accepts_valid_login() {
        assert!(Login::try_new("s27b_ivanov").is_ok());
        assert!(Login::try_new("t_ivanova").is_ok());
        assert!(Login::try_new("a").is_ok());
        assert!(Login::try_new("user.name-1").is_ok());
    }

    #[test]
    fn try_new_lowercases_input() {
        let login = Login::try_new("S27B_ivanov").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn try_new_trims_whitespace() {
        let login = Login::try_new("  s27b_ivanov  ").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn try_new_rejects_empty() {
        assert!(matches!(
            Login::try_new(""),
            Err(DomainError::InvalidLoginFormat)
        ));
        assert!(matches!(
            Login::try_new("   "),
            Err(DomainError::InvalidLoginFormat)
        ));
    }

    #[test]
    fn try_new_rejects_forbidden_chars() {
        for bad in ["user name", "user@name", "юзер", "user!", "user/name"] {
            assert!(matches!(
                Login::try_new(bad),
                Err(DomainError::InvalidLoginFormat)
            ));
        }
    }

    #[test]
    fn try_new_rejects_too_long() {
        let long = "a".repeat(101);
        assert!(matches!(
            Login::try_new(&long),
            Err(DomainError::InvalidLoginFormat)
        ));
    }

    #[test]
    fn try_new_accepts_max_length() {
        let max = "a".repeat(100);
        assert!(Login::try_new(&max).is_ok());
    }

    #[test]
    fn email_derives_school_address() {
        init_settings();
        let login = Login::try_new("s27b_ivanov").unwrap();
        assert_eq!(login.email(), "s27b_ivanov@179.ru");
    }

    #[test]
    fn from_email_parses_school_address() {
        init_settings();
        let login = Login::from_email("s27b_ivanov@179.ru").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn from_email_is_case_insensitive() {
        init_settings();
        let login = Login::from_email("S27B_ivanov@179.RU").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn from_email_rejects_foreign_domain() {
        init_settings();
        assert!(matches!(
            Login::from_email("user@gmail.com"),
            Err(DomainError::InvalidEmailFormat)
        ));
    }

    #[test]
    fn from_email_rejects_missing_domain() {
        init_settings();
        assert!(matches!(
            Login::from_email("s27b_ivanov"),
            Err(DomainError::InvalidEmailFormat)
        ));
    }

    #[test]
    fn from_email_rejects_empty_local_part() {
        init_settings();
        assert!(matches!(
            Login::from_email("@179.ru"),
            Err(DomainError::InvalidLoginFormat)
        ));
    }

    #[test]
    fn from_identifier_accepts_login() {
        let login = Login::from_identifier("s27b_ivanov").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn from_identifier_accepts_email() {
        init_settings();
        let login = Login::from_identifier("s27b_ivanov@179.ru").unwrap();
        assert_eq!(login.as_str(), "s27b_ivanov");
    }

    #[test]
    fn display_matches_login_string() {
        let login = Login::try_new("s27b_ivanov").unwrap();
        assert_eq!(login.to_string(), "s27b_ivanov");
    }
}
