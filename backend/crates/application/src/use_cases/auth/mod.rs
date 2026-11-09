//! Authentication use cases.
//!
//! This module contains use cases for user authentication:
//! - `login`: authenticate a user with login + password and issue a session token.
//! - `email_login`: the site's sign-in by school email: password, or on the first
//!   login an emailed code followed by a mandatory new password.
//!
//! Dependencies: `domain` crate (entities, repositories, ports, errors).
//! Guarantees: All methods return `Result`. No panics.

pub mod email_login;
pub mod login;
pub mod one_time_code_service;
pub use email_login::EmailLoginService;
pub use one_time_code_service::OneTimeCodeService;

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
