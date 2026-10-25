//! Cryptographically secure implementation of `SessionTokenGenerator`.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use domain::ports::session_security::SessionTokenGenerator;
use rand::{rngs::OsRng, RngCore};

/// Generates URL-safe opaque session tokens.
pub struct SecureSessionTokenGenerator;

impl SessionTokenGenerator for SecureSessionTokenGenerator {
    fn generate_session_token(&self) -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }
}
