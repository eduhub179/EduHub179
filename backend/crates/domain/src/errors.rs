//! Domain-level errors.
//!
//! Guarantees: All possible business logic errors are enumerated here.
//! This ensures at compile time that no erroneous state is ignored
//! (using `Result` is mandatory). No panics are allowed.

use std::fmt;

/// Base domain error. Contains no implementation details (e.g., SQL errors).
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    /// User with the specified ID or email was not found.
    UserNotFound,

    /// Uniqueness violation (e.g., registration with an existing email).
    EmailAlreadyExists,

    /// Invalid email format.
    InvalidEmailFormat,

    /// Invalid name format (e.g., empty or whitespace-only string).
    InvalidNameFormat,

    /// Attempt to perform an action not allowed for the current role.
    InsufficientPermissions,

    /// User account is blocked or inactive.
    UserIsInactive,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Note: These are for logging. The presentation layer should map
        // these to localized Russian messages for the end user.
        match self {
            DomainError::UserNotFound => write!(f, "User not found"),
            DomainError::EmailAlreadyExists => write!(f, "Email already exists"),
            DomainError::InvalidEmailFormat => write!(f, "Invalid email format"),
            DomainError::InvalidNameFormat => write!(f, "Invalid name format"),
            DomainError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            DomainError::UserIsInactive => write!(f, "User account is inactive"),
        }
    }
}

impl std::error::Error for DomainError {}