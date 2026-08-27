//! JWT implementation of the `TokenIssuer` trait.
//!
//! Dependencies: `jsonwebtoken` crate, `domain` crate (ports + entities + errors).
//! Guarantees:
//! - All methods return `Result`. No panics.
//! - Uses HS256 (symmetric, shared secret from `JWT_SECRET` in `.env`).
//! - Token lifetime is configurable (default 15 minutes for MVP).
//! - `role` is embedded in the token for fast authorization without a DB round-trip.
//!
//! SECURITY NOTE: The secret is stored as `Vec<u8>` in memory. For production,
//! consider rotating secrets and using RS256 (asymmetric) if multiple services
//! need to verify tokens.

use chrono::Utc;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::ports::auth::{TokenClaims, TokenIssuer};
use domain::value_objects::role::UserRole;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// JWT claims embedded in the session token.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject: the user id.
    sub: String,
    /// User role (for authorization without a DB round-trip).
    role: String,
    /// Expiry time (Unix seconds).
    exp: usize,
    /// Issued at (Unix seconds).
    iat: usize,
}

/// JWT token issuer (HS256).
pub struct JwtTokenIssuer {
    secret: Vec<u8>,
    /// Token lifetime in seconds (default 900 = 15 minutes).
    ttl_seconds: i64,
}

impl JwtTokenIssuer {
    /// Creates a new issuer.
    ///
    /// `secret` should come from `JWT_SECRET` in `.env` (minimum 32 bytes).
    /// `ttl_seconds` is the token lifetime (e.g., 900 for 15 minutes).
    pub fn new(secret: String, ttl_seconds: i64) -> Self {
        Self {
            secret: secret.into_bytes(),
            ttl_seconds,
        }
    }
}

impl TokenIssuer for JwtTokenIssuer {
    fn issue(&self, user: &User) -> Result<String, DomainError> {
        let now = Utc::now().timestamp() as usize;
        let claims = Claims {
            sub: user.id.to_string(),
            role: user.role.to_string(),
            exp: now + self.ttl_seconds as usize,
            iat: now,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
        .map_err(|_| DomainError::InternalError)
    }

    fn verify(&self, token: &str) -> Result<TokenClaims, DomainError> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &Validation::default(),
        )
        .map_err(|_| DomainError::InvalidCredentials)?;

        let user_id =
            Uuid::parse_str(&data.claims.sub).map_err(|_| DomainError::InvalidCredentials)?;
        let role =
            UserRole::from_str(&data.claims.role).map_err(|_| DomainError::InvalidCredentials)?;

        Ok(TokenClaims { user_id, role })
    }
}
