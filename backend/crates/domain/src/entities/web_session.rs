//! Web session entity for cookie-based authentication.
//!
//! Stores only hashed credentials for security. The raw cookie token must
//! never enter this entity or persistence layer.

use crate::errors::DomainError;
use crate::value_objects::csrf_token_hash::CsrfTokenHash;
use crate::value_objects::role::UserRole;
use crate::value_objects::session_token_hash::SessionTokenHash;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Input model for creating a new web session.
#[derive(Debug, Clone)]
pub struct NewWebSession {
    pub user_id: Uuid,
    pub session_token_hash: SessionTokenHash,
    pub csrf_token_hash: Option<CsrfTokenHash>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_persistent: bool,
    pub expires_at: DateTime<Utc>,
}

impl NewWebSession {
    /// Creates a validated creation model.
    pub fn try_new(
        user_id: Uuid,
        session_token_hash: SessionTokenHash,
        csrf_token_hash: Option<CsrfTokenHash>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        is_persistent: bool,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            user_id,
            session_token_hash,
            csrf_token_hash,
            ip_address,
            user_agent,
            is_persistent,
            expires_at,
        })
    }
}

/// Active web session used by auth middleware.
#[derive(Debug, Clone)]
pub struct WebSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub role: UserRole,
    pub session_token_hash: SessionTokenHash,
    pub csrf_token_hash: Option<CsrfTokenHash>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_persistent: bool,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
