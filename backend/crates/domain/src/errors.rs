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

    /// Class with the specified ID was not found.
    ClassNotFound,
    /// Invalid class letter (not 'б', 'в', or 'и').
    InvalidClassLetter,
    /// Class with the same (graduation_year, class_letter) already exists (unique violation).
    ClassAlreadyExists,

    /// Graduation year is out of acceptable bounds (e.g., < 1900 or > 2200).
    InvalidGraduationYear,

    /// Subject with the specified ID was not found.
    SubjectNotFound,
    /// Subject with the same name already exists (unique violation).
    SubjectAlreadyExists,
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
            DomainError::ClassNotFound => write!(f, "Class not found"),
            DomainError::InvalidClassLetter => write!(f, "Invalid class letter"),
            DomainError::ClassAlreadyExists => write!(f, "Class already exists"),
            DomainError::InvalidGraduationYear => write!(f, "Invalid graduation year"),
            DomainError::SubjectNotFound => write!(f, "Subject not found"),
            DomainError::SubjectAlreadyExists => write!(f, "Subject already exists"),
        }
    }
}

impl std::error::Error for DomainError {}