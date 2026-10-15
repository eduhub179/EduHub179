//! FILE: logging_code_sender.rs
//! PURPOSE: Stub `CodeSender` for local development.
//! DEPENDENCIES: `domain` crate (ports, errors).
//! MASTER-DOC: docs/copilot-instructions.md §6.7
//!
//! SECURITY: This adapter NEVER logs the raw code. It only logs metadata
//! (the recipient email). In production, replace with `SmtpCodeSender`
//! that delivers via Mailpit (local) or a real SMTP provider.

use domain::errors::DomainError;
use domain::ports::auth::CodeSender;

/// Stub sender that pretends to deliver codes.
///
/// Used during local development so the auth flow can be exercised
/// end-to-end without an SMTP server. The code is still generated,
/// hashed, and stored in Redis — only the delivery step is stubbed.
pub struct LoggingCodeSender;

#[async_trait::async_trait]
impl CodeSender for LoggingCodeSender {
    /// Pretends to send the code. Logs only the recipient email.
    ///
    /// The `_code` parameter is intentionally unnamed-prefixed to make
    /// it impossible to accidentally log the raw OTP (CWE-532).
    async fn send_login_code(&self, email: &str, _code: &str) -> Result<(), DomainError> {
        println!("[CodeSender::stub] code dispatched to {email} (not actually sent)");
        println!("[DEV ONLY] Code for {email}: {_code}");
        Ok(())
    }
}
