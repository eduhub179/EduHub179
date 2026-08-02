//! Repository trait for user persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::user::User;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the user storage.
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    /// Fetches a user by their unique identifier.
    /// Fail-safe: Returns `UserNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError>;

    /// Fetches a user by email (used during authentication).
    async fn get_by_email(&self, email: &str) -> Result<User, DomainError>;

    /// Fetches a list of active students in a specific class, sorted by last name.
    ///
    /// Performance: The implementation in the database should rely on the partial index:
    /// `CREATE INDEX idx_users_class_last_name ON users (class_id, last_name)
    ///  WHERE role = 'student' AND is_active = TRUE;`
    async fn get_active_students_by_class(&self, class_id: Uuid) -> Result<Vec<User>, DomainError>;

    /// Saves or updates a user.
    async fn save(&self, user: User) -> Result<User, DomainError>;
}