//! Authentication ports: everything the auth subsystem needs from the
//! outside world, grouped in one cohesive module.
//!
//! Implemented in the `infrastructure` crate (PostgreSQL, JWT, email stub).
//! Consumed by `application` use cases.
//!
//! Guarantees: all fallible methods return `Result`; no panics.
//!
//! SECURITY NOTE: password hashes NEVER cross this boundary. Hashing and
//! verification happen inside the implementing adapter (`CredentialsStore`);
//! only raw passwords (transiently) and boolean results cross it.

use crate::entities::user::User;
use crate::errors::DomainError;
use crate::value_objects::role::UserRole;
use uuid::Uuid;

// ============================================================================
// SESSION TOKENS (JWT)
// ============================================================================

/// Claims extracted from a valid session token (for authorization).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    /// The authenticated user's id.
    pub user_id: Uuid,
    /// The user's role (authorization without a DB round-trip).
    pub role: UserRole,
}

/// Issues and verifies signed session tokens.
pub trait TokenIssuer: Send + Sync {
    /// Issues a signed token (JWT) for the given user.
    /// Returns `Err(DomainError::InternalError)` if token creation fails.
    fn issue(&self, user: &User) -> Result<String, DomainError>;

    /// Verifies a token and extracts its claims.
    /// Returns `Err(DomainError::InvalidCredentials)` for expired,
    /// malformed, or wrongly signed tokens.
    fn verify(&self, token: &str) -> Result<TokenClaims, DomainError>;
}

// ============================================================================
// PASSWORD STORAGE AND VERIFICATION
// ============================================================================

/// Manages user passwords.
///
/// Deliberately separated from `UserRepository` so the domain `User` entity
/// never carries password data.
///
/// SECURITY NOTE: the stored hash NEVER crosses this boundary. Hashing and
/// verification happen inside the implementing adapter; only raw passwords
/// (transiently) and boolean results cross it. This confines the persistent
/// secret to the infrastructure layer.
#[async_trait::async_trait]
pub trait CredentialsStore: Send + Sync {
    /// Checks a raw password against the stored hash.
    ///
    /// Returns `Ok(false)` if the user does not exist, has no password set,
    /// or the password does not match (fail-safe: no information leak).
    async fn verify_password(&self, user_id: Uuid, raw_password: &str)
        -> Result<bool, DomainError>;

    /// Hashes the raw password and stores it (replacing any existing one).
    ///
    /// Returns `Err(DomainError::UserNotFound)` if the user does not exist.
    async fn set_password(&self, user_id: Uuid, raw_password: &str) -> Result<(), DomainError>;
}

// ============================================================================
// ONE-TIME LOGIN / RECOVERY CODES
// ============================================================================

/// Stores and consumes one-time authentication codes.
///
/// Codes are stored hashed; at most one active code per user.
#[async_trait::async_trait]
pub trait AuthCodeStore: Send + Sync {
    /// Stores (or replaces) a code for the user with a TTL in seconds.
    /// The implementation hashes the code before persisting it.
    async fn store(&self, user_id: Uuid, code: &str, ttl_seconds: i64) -> Result<(), DomainError>;

    /// Verifies a code and consumes it on success (one-time use).
    /// Returns `Ok(false)` if the code is wrong, expired, or absent.
    async fn verify_and_consume(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError>;
}

/// Delivers a login/recovery code to the user's email.
///
/// MVP implementation logs the code; a real SMTP/transactional adapter comes later.
#[async_trait::async_trait]
pub trait CodeSender: Send + Sync {
    /// Sends the given code to the given email address.
    async fn send_login_code(&self, email: &str, code: &str) -> Result<(), DomainError>;
}

/// Generates one-time authentication codes.
pub trait CodeGenerator: Send + Sync {
    /// Returns a new one-time code (e.g. a 6-digit numeric string).
    fn generate(&self) -> String;
}
