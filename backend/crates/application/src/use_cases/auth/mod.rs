//! Authentication use cases.
//!
//! This module contains use cases for user authentication:
//! - `login`: authenticate a user with login + password and issue a session token.
//!
//! Dependencies: `domain` crate (entities, repositories, ports, errors).
//! Guarantees: All methods return `Result`. No panics.

pub mod login;

use domain::entities::user::User;

/// Result of a successful authentication: a session token plus the user.
///
/// Deliberately lives in the application layer (not the domain), because a
/// "session" is an application/infrastructure concern, not a business entity.
#[derive(Debug, Clone)]
pub struct AuthSession {
    /// Signed session token (JWT) to be returned to the client.
    pub token: String,
    /// The authenticated user.
    pub user: User,
}
