//! FILE: smtp_code_sender.rs
//! PURPOSE: SMTP implementation of the `CodeSender` port for delivering one-time login codes.
//! DEPENDENCIES: `lettre` crate, `domain` crate (ports, errors).
//! MASTER-DOC: docs/copilot-instructions.md §6.7
//!
//! Guarantees:
//! - All methods return `Result`. No panics.
//! - SMTP errors are mapped to `DomainError::InternalError` to prevent
//!   leaking infrastructure details (like server responses) to the domain layer.
//! - The adapter does not depend on the global `Config` struct; it receives
//!   only the specific primitives it needs, keeping it loosely coupled.
//!
//! SECURITY: The raw code is included in the email body. The email is sent
//! over TLS (configured via lettre's `relay`). Delivery failures are logged
//! via `tracing` but never exposed to the caller.

use domain::errors::DomainError;
use domain::ports::auth::CodeSender;
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

/// SMTP-based code sender.
///
/// Encapsulates the SMTP transport and the sender address.
/// Designed to be instantiated once at the composition root and shared
/// via `Arc<dyn CodeSender>`.
pub struct SmtpCodeSender {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_email: String,
}

impl SmtpCodeSender {
    /// Creates a new SMTP sender from individual configuration fields.
    ///
    /// # Fail-safe behavior
    /// Returns `Err(DomainError::InternalError)` if the SMTP transport cannot
    /// be initialized (e.g., invalid host for TLS). The caller (composition root)
    /// should handle this at startup and fail loudly.
    pub fn try_new(
        host: String,
        port: u16,
        user: String,
        password: String,
        from_email: String,
    ) -> Result<Self, DomainError> {
        let creds = Credentials::new(user, password);

        // `relay` sets up STARTTLS (standard for port 587).
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .map_err(|_| DomainError::InternalError)?
            .credentials(creds)
            .port(port)
            .build();

        Ok(Self { mailer, from_email })
    }
}

#[async_trait::async_trait]
impl CodeSender for SmtpCodeSender {
    /// Sends the login code via email.
    ///
    /// # Fail-safe behavior
    /// - Invalid `from_email` (misconfigured at startup) -> `InternalError`.
    /// - Invalid recipient `email` -> `InvalidEmailFormat`.
    /// - SMTP delivery failure -> `InternalError` (logged internally, but
    ///   details are hidden from the domain layer to prevent information leakage).
    async fn send_login_code(&self, email: &str, code: &str) -> Result<(), DomainError> {
        // Parse addresses. Invalid formats are mapped to domain errors.
        let from_addr: Mailbox = self
            .from_email
            .parse()
            .map_err(|_| DomainError::InternalError)?;

        let to_addr: Mailbox = email.parse().map_err(|_| DomainError::InvalidEmailFormat)?;

        // HTML template for the email.
        let html_body = format!(
            r#"<div style="font-family: sans-serif; max-width: 400px; margin: 0 auto;">
                <h2>Ваш код для входа</h2>
                <p>Используйте этот код для авторизации в боте или на сайте:</p>
                <div style="font-size: 32px; font-weight: bold; letter-spacing: 5px; text-align: center; background: #f4f4f4; padding: 20px; border-radius: 8px;">
                    {}
                </div>
                <p style="color: #666; font-size: 14px;">Код действителен в течение 10 минут. Если вы не запрашивали код, просто проигнорируйте это письмо.</p>
            </div>"#,
            code
        );

        let email_msg = Message::builder()
            .from(from_addr)
            .to(to_addr)
            .subject("Код подтверждения EduHub179")
            .header(ContentType::TEXT_HTML)
            .body(html_body)
            .map_err(|_| DomainError::InternalError)?;

        // Send the email. Map lettre errors to InternalError.
        self.mailer.send(email_msg).await.map_err(|e| {
            // Log the actual error for debugging/monitoring,
            // but return a generic domain error to the caller.
            tracing::error!("SMTP send failed: {:?}", e);
            DomainError::InternalError
        })?;

        Ok(())
    }
}
