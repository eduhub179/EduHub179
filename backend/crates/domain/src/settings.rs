//! Deployment-wide settings (per-deployment school constants).
//!
//! The domain never reads the environment itself: the composition root
//! (`bin/main.rs`) loads `Config` and injects the values once via [`init`].
//! Readers access the shared instance via [`get`].
//!
//! Dependencies: only `crate::errors::DomainError` and `std::sync::OnceLock`.
//! Guarantees:
//! - `try_new` validates strictly and returns `Err` on invalid input — no
//!   silent guessing (fail-fast at the composition root).
//! - `init` is idempotent: the first call wins, later calls are ignored.
//! - `get` panics if `init` was never called: settings initialization is a
//!   mandatory startup step, and its absence is a programmer error surfaced
//!   loudly with an actionable message.

use crate::errors::DomainError;
use std::sync::OnceLock;

/// Deployment-wide school constants.
///
/// A plain data carrier: fields are public, invariants live in
/// [`Settings::try_new`]. New per-deployment constants become new fields here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Settings {
    /// Organization email domain, e.g. "@179.ru".
    /// Guaranteed to start with '@' and contain at least one more character.
    pub org_email_domain: String,
}

impl Settings {
    /// Creates settings with strict validation (fail-safe, no guessing).
    ///
    /// Normalization is limited to semantics-free cleanup: trim + lowercase.
    /// Returns `Err(DomainError::InvalidOrgEmailDomain)` if the value is
    /// empty, is exactly "@", or does not start with '@' — the caller must
    /// provide the domain in the exact form it should appear in emails.
    pub fn try_new(org_email_domain: String) -> Result<Self, DomainError> {
        let v = org_email_domain.trim().to_lowercase();
        if !v.starts_with('@') || v.chars().count() < 2 {
            return Err(DomainError::InvalidOrgEmailDomain);
        }
        Ok(Self {
            org_email_domain: v,
        })
    }

    /// Mock constructor for tests: a valid instance with the documented test
    /// domain. Production code must use [`Settings::try_new`] with a value
    /// from `Config`, never this.
    pub fn mock() -> Self {
        Self {
            org_email_domain: "@179.ru".to_string(),
        }
    }
}

/// The injected deployment settings; set once at boot by `init`.
static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Injects deployment settings. Called once from `main` right after
/// `Config::load()`; later calls are silently ignored (fail-safe).
pub fn init(settings: Settings) {
    let _ = SETTINGS.set(settings);
}

/// Global read access to the deployment settings.
///
/// # Panics
/// If [`init`] was never called: running without settings is a programmer
/// error (a missed mandatory startup step), not a business error. The panic
/// message tells exactly how to fix it.
pub fn get() -> &'static Settings {
    SETTINGS.get().expect(
        "domain::settings not initialized: call domain::settings::init(...) in the \
         composition root (bin/src/main.rs) right after Config::load(), or \
         domain::settings::init(Settings::mock()) in unit tests",
    )
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain settings`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Initializes the deployment settings once (idempotent) so tests can
    /// read the mock domain.
    fn init_settings() {
        crate::settings::init(crate::settings::Settings::mock());
    }

    #[test]
    fn try_new_normalizes_case_and_trims() {
        let s = Settings::try_new("  @179.RU  ".to_string()).unwrap();
        assert_eq!(s.org_email_domain, "@179.ru");
    }

    #[test]
    fn try_new_rejects_missing_at() {
        assert!(Settings::try_new("179.ru".to_string()).is_err());
    }

    #[test]
    fn try_new_rejects_bare_at() {
        assert!(Settings::try_new("@".to_string()).is_err());
    }

    #[test]
    fn try_new_rejects_empty() {
        assert!(Settings::try_new("   ".to_string()).is_err());
    }

    #[test]
    fn get_returns_injected_settings() {
        init_settings();
        assert_eq!(get().org_email_domain, "@179.ru");
    }

    #[test]
    fn init_is_idempotent() {
        init_settings();
        init_settings(); // Second call is silently ignored
        assert_eq!(get().org_email_domain, "@179.ru");
    }
}
