//! User entity.
//!
//! Invariants:
//! - `login` must pass basic format validation upon creation.
//! - `is_active` defaults to `true` for new users.
//! - Names cannot be empty strings.

use crate::errors::DomainError;
use crate::value_objects::login::Login;
use crate::value_objects::role::UserRole;
use uuid::Uuid;

/// Representation of a user in the domain model.
/// Does not contain sensitive data (e.g., password_hash),
/// as it is handled at the infrastructure (authentication) layer.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    /// Unique user identifier (UUID v4).
    pub id: Uuid,

    /// Unique login used for authentication.
    pub login: String,

    /// User role defining access rights.
    pub role: UserRole,

    /// Last name (for sorting and display in class lists).
    pub last_name: String,

    /// First name.
    pub first_name: String,

    /// Middle name (optional).
    pub middle_name: Option<String>,

    /// Activity flag. Inactive users cannot log in.
    pub is_active: bool,

    /// Class identifier (only applicable for Student role).
    pub class_id: Option<Uuid>,
}

impl User {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err` if login is invalid or names are empty.
    /// This prevents invalid entities from reaching the repository.
    pub fn try_new(
        id: Uuid,
        login: String,
        role: UserRole,
        last_name: String,
        first_name: String,
        middle_name: Option<String>,
        class_id: Option<Uuid>,
    ) -> Result<Self, DomainError> {
        // Basic login validation.
        if login.is_empty() {
            return Err(DomainError::InvalidLoginFormat);
        }

        if last_name.trim().is_empty() || first_name.trim().is_empty() {
            return Err(DomainError::InvalidNameFormat);
        }

        Ok(Self {
            id,
            login,
            role,
            last_name,
            first_name,
            middle_name,
            is_active: true, // Invariant: new users are active by default
            class_id,
        })
    }

    /// Delegates role check to the Value Object for clean business logic.
    pub fn is_teacher(&self) -> bool {
        self.role.is_teacher()
    }

    /// Delegates role check to the Value Object.
    pub fn is_admin(&self) -> bool {
        self.role.is_admin()
    }

    /// Checks if the user is allowed to be assigned to a class.
    pub fn can_have_class(&self) -> bool {
        self.role.is_student()
    }
    pub fn get_email(&self) -> String {
        Login::try_new(&self.login)
            .map(|login| login.email())
            .unwrap()
    }
}
