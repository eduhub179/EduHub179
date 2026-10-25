//!
// Authentication adapters: password hashing, JWT, code sending/generation.
pub mod argon2_password_hasher;
pub mod jwt_token_issuer;
pub mod logging_code_sender;
pub mod numeric_code_generator;
pub mod password_hasher;
pub mod secure_session_token_generator;
pub mod smtp_code_sender;
pub mod hmac_session_token_hasher;

pub use argon2_password_hasher::Argon2PasswordHasher;
pub use jwt_token_issuer::JwtTokenIssuer;
pub use logging_code_sender::LoggingCodeSender;
pub use numeric_code_generator::NumericCodeGenerator;
pub use password_hasher::PasswordHasher;
pub use secure_session_token_generator::SecureSessionTokenGenerator;
pub use smtp_code_sender::SmtpCodeSender;
pub use hmac_session_token_hasher::HmacSessionTokenHasher;
