//! Value Object for user role.
//!
//! Guarantees: An instance can only be created from a predefined set.
//! Any attempt to parse an unknown string from the DB will return `Err`,
//! preventing "garbage" roles from entering the system.

use crate::errors::DomainError;
use std::str::FromStr;

/// User role in the system.
/// Corresponds to the `user_role` ENUM in PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserRole {
    /// Student: can view own homework, plusnik, propose oral homework.
    Student,
    /// Teacher: can create homework, grant pluses, moderate oral homework.
    Teacher,
    /// Admin: manages users, classes, subjects, and global settings.
    Admin,
}

impl UserRole {
    /// Fail-safe: Explicit check for Teacher role.
    /// Use this instead of `self == UserRole::Teacher` for better readability.
    pub fn is_teacher(&self) -> bool {
        matches!(self, UserRole::Teacher)
    }

    /// Fail-safe: Explicit check for Admin role.
    pub fn is_admin(&self) -> bool {
        matches!(self, UserRole::Admin)
    }

    /// Fail-safe: Explicit check for Student role.
    pub fn is_student(&self) -> bool {
        matches!(self, UserRole::Student)
    }

    /// Checks if the role has staff privileges (Teacher or Admin).
    /// Useful for features available to both, but not to students.
    pub fn is_staff(&self) -> bool {
        matches!(self, UserRole::Teacher | UserRole::Admin)
    }
}

impl FromStr for UserRole {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "student" => Ok(UserRole::Student),
            "teacher" => Ok(UserRole::Teacher),
            "admin" => Ok(UserRole::Admin),
            _ => Err(DomainError::InvalidNameFormat), // Reusing format error for simplicity, or create InvalidRole
        }
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            UserRole::Student => "student",
            UserRole::Teacher => "teacher",
            UserRole::Admin => "admin",
        };
        write!(f, "{}", s)
    }
}