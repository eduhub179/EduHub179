//! Infrastructure configuration.
//!
//! Loads environment variables into a strictly typed struct.
//! This keeps `.env` parsing logic out of the composition root (`bin`)
//! and encapsulates infrastructure concerns (DB URLs, secrets) where they belong.
//!
//! Dependencies: `dotenvy`, `std::env`.
//! Guarantees: required fields are validated at load time; the application
//! fails fast at startup with a clear message if a required variable is missing.

use dotenvy::dotenv;
use std::env;

/// Application configuration derived from environment variables.
///
/// Invariants:
/// - `database_url`, `jwt_secret`, `jwt_ttl_seconds` are required (no fallbacks).
/// - `server_host`, `server_port`, `org_email_domain` have sensible defaults.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string (required).
    pub database_url: String,
    /// JWT signing secret (required, minimum 32 bytes for HS256).
    pub jwt_secret: String,
    /// JWT token lifetime in seconds (required).
    pub jwt_ttl_seconds: i64,
    /// Server bind host (default: 127.0.0.1).
    pub server_host: String,
    /// Server bind port (default: 8080).
    pub server_port: u16,
    /// Organization email domain for login validation (default: @179.ru).
    pub org_email_domain: String,
}

impl Config {
    /// Loads configuration from the environment.
    ///
    /// # Fail-fast behavior
    /// Panics with a clear message if a required variable is missing or
    /// malformed. This is acceptable for the composition root (`main.rs`)
    /// because the app cannot function without these values.
    pub fn load() -> Self {
        // Load .env if present; ignore the error when running in Docker/CI,
        // where variables come from the orchestrator instead.
        dotenv().ok();

        let database_url =
            env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env or OS environment");

        let jwt_secret =
            env::var("JWT_SECRET").expect("JWT_SECRET must be set in .env or OS environment");

        // JWT_TTL_SECONDS is strictly required. No fallbacks.
        let jwt_ttl_seconds = env::var("JWT_TTL_SECONDS")
            .expect("JWT_TTL_SECONDS must be set in .env or OS environment")
            .parse::<i64>()
            .expect("JWT_TTL_SECONDS must be a valid integer (e.g., 900)");

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .expect("SERVER_PORT must be a valid u16");

        let org_email_domain =
            env::var("ORG_EMAIL_DOMAIN").unwrap_or_else(|_| "@179.ru".to_string());

        Self {
            database_url,
            jwt_secret,
            jwt_ttl_seconds,
            server_host,
            server_port,
            org_email_domain,
        }
    }

    /// Creates a mock configuration for testing.
    /// Allows tests to bypass the OS environment entirely.
    pub fn mock() -> Self {
        Self {
            database_url: "postgres://mock:test@localhost/mock_db".to_string(),
            jwt_secret: "test_secret_at_least_32_bytes_long_for_hs256!".to_string(),
            jwt_ttl_seconds: 900,
            server_host: "127.0.0.1".to_string(),
            server_port: 8080,
            org_email_domain: "@179.ru".to_string(),
        }
    }
}
