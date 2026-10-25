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

use crate::notifications::WebPushSettings;
use crate::storage::S3Settings;

/// Application configuration derived from environment variables.
///
/// Invariants:
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string (required).
    pub database_url: String,
    /// JWT signing secret (required, minimum 32 bytes for HS256).
    pub jwt_secret: String,
    /// JWT token lifetime in seconds (required).
    pub jwt_ttl_seconds: u64,
    /// Secret for deterministic HMAC hashing of cookie session tokens.
    pub session_hash_secret: String,
    /// Server bind host (default: 127.0.0.1).
    pub server_host: String,
    /// Server bind port (default: 8080).
    pub server_port: u16,
    /// Organization email domain for login validation (default: @179.ru).
    pub org_email_domain: String,
    /// Redis URL for sessions and auth codes (required).
    pub redis_url: String,
    /// Telegram bot token (required for the bot binary).
    pub telegram_bot_token: String,
    /// Where the bot forwards updates it does not handle itself: class chat messages, homework sent
    /// in DM by a logged-in user, agent buttons (optional; e.g. http://127.0.0.1:8091/telegram/update).
    pub agent_url: Option<String>,
    /// Shared secret sent to the agent in `X-Agent-Key` (optional).
    pub agent_key: Option<String>,

    /// SMTP service
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_from_email: String,

    /// Object storage for homework files (MinIO / Yandex Object Storage).
    /// `None` if `S3_ENDPOINT` is not set: file storage is off.
    pub s3: Option<S3Settings>,

    /// Browser push notifications (VAPID). `None` if `VAPID_PRIVATE_KEY` is not set.
    pub web_push: Option<WebPushSettings>,
}

impl Config {
    pub fn load() -> Self {
        dotenv().ok();

        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");
        let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set in .env");
        let jwt_ttl_seconds = env::var("JWT_TTL_SECONDS")
            .expect("JWT_TTL_SECONDS must be set in .env")
            .parse::<u64>()
            .expect("JWT_TTL_SECONDS must be a valid integer");
        let session_hash_secret = env::var("SESSION_HASH_SECRET")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| jwt_secret.clone());
        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .expect("SERVER_PORT must be a valid u16");
        let org_email_domain =
            env::var("ORG_EMAIL_DOMAIN").unwrap_or_else(|_| "@179.ru".to_string());

        // New fields — required for the bot binary.
        let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set in .env");
        let telegram_bot_token =
            env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN must be set in .env");
        let agent_url = env::var("AGENT_URL").ok().filter(|v| !v.is_empty());
        let agent_key = env::var("AGENT_KEY").ok().filter(|v| !v.is_empty());

        let smtp_host = env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string());
        let smtp_port = env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse::<u16>()
            .expect("SMTP_PORT must be a valid u16");
        let smtp_user = env::var("SMTP_USER").expect("SMTP_USER must be set in .env");
        let smtp_password = env::var("SMTP_PASSWORD").expect("SMTP_PASSWORD must be set in .env");
        let smtp_from_email = env::var("SMTP_FROM_EMAIL").unwrap_or_else(|_| smtp_user.clone());
        let s3 = Self::load_s3();
        let web_push = Self::load_web_push();
        Self {
            database_url,
            jwt_secret,
            jwt_ttl_seconds,
            session_hash_secret,
            server_host,
            server_port,
            org_email_domain,
            redis_url,
            telegram_bot_token,
            agent_url,
            agent_key,
            smtp_host,
            smtp_port,
            smtp_user,
            smtp_password,
            smtp_from_email,
            s3,
            web_push,
        }
    }

    /// `VAPID_PRIVATE_KEY` turns browser push on; then `VAPID_SUBJECT` (a contact for push
    /// services, `mailto:...` or `https://...`) is required — fail fast at startup.
    fn load_web_push() -> Option<WebPushSettings> {
        let private_key = env::var("VAPID_PRIVATE_KEY").ok().filter(|v| !v.trim().is_empty())?;
        let subject = env::var("VAPID_SUBJECT")
            .ok()
            .filter(|v| v.starts_with("mailto:") || v.starts_with("https://"))
            .expect("VAPID_SUBJECT (mailto:... or https://...) must be set when VAPID_PRIVATE_KEY is set");
        Some(WebPushSettings {
            private_key,
            subject,
        })
    }

    /// `S3_ENDPOINT` turns file storage on; then the keys and the bucket are required
    /// (fail fast at startup instead of failing on the first upload).
    fn load_s3() -> Option<S3Settings> {
        let endpoint = env::var("S3_ENDPOINT").ok().filter(|v| !v.trim().is_empty())?;
        let required = |name: &str| {
            env::var(name)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| panic!("{name} must be set in .env when S3_ENDPOINT is set"))
        };

        Some(S3Settings {
            endpoint,
            access_key: required("S3_ACCESS_KEY"),
            secret_key: required("S3_SECRET_KEY"),
            bucket: required("S3_BUCKET"),
            region: env::var("S3_REGION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "us-east-1".to_string()),
        })
    }

    pub fn mock() -> Self {
        Self {
            database_url: "postgres://mock:test@localhost/mock_db".to_string(),
            jwt_secret: "test_secret_at_least_32_bytes_long_for_hs256!".to_string(),
            jwt_ttl_seconds: 900,
            session_hash_secret: "test_session_hash_secret_32_bytes_minimum!".to_string(),
            server_host: "127.0.0.1".to_string(),
            server_port: 8080,
            org_email_domain: "@179.ru".to_string(),
            redis_url: "redis://127.0.0.1:6379".to_string(),
            telegram_bot_token: "mock_telegram_bot_token".to_string(),
            agent_url: None,
            agent_key: None,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_user: "noreply@179.ru".to_string(),
            smtp_password: "smtp_password".to_string(),
            smtp_from_email: "noreply@179.ru".to_string(),
            s3: None,
            web_push: None,
        }
    }
}
