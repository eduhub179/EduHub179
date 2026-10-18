//! FILE: one_time_code_service.rs
//! PURPOSE: Shared lifecycle of one-time login/recovery codes:
//!          generate -> store (hashed, TTL) -> deliver, and verify -> consume.
//! DEPENDENCIES: domain (ports::auth, errors), uuid.
//! MASTER-DOC: domain/src/ports/auth.rs owns the code-lifecycle contract.

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::auth::{AuthCodeStore, CodeGenerator, CodeSender};
use uuid::Uuid;

/// Default lifetime of a one-time code, in seconds.
///
/// Single source of truth: Telegram, web login and recovery must share it,
/// otherwise TTL rules silently drift apart per channel.
pub const DEFAULT_CODE_TTL_SECONDS: u64 = 600;

/// Shared lifecycle of one-time authentication codes.
///
/// Entry points (Telegram, web login, password recovery) depend on this
/// instead of re-implementing "generate/store/send/verify", so code rules
/// stay identical across channels.
///
/// Fail-safe: every fallible step returns `Result`; a failed delivery is
/// surfaced to the caller and never swallowed here.
pub struct OneTimeCodeService {
    generator: Arc<dyn CodeGenerator>,
    store: Arc<dyn AuthCodeStore>,
    sender: Arc<dyn CodeSender>,
}

impl OneTimeCodeService {
    pub fn new(
        generator: Arc<dyn CodeGenerator>,
        store: Arc<dyn AuthCodeStore>,
        sender: Arc<dyn CodeSender>,
    ) -> Self {
        Self {
            generator,
            store,
            sender,
        }
    }

    /// Issues a code for `user_id` and delivers it to `email`.
    ///
    /// Order matters: store first, then send. On delivery failure the stored
    /// code just expires by TTL, and a retry overwrites it (one active code
    /// per user by construction) — so the flow is always recoverable.
    pub async fn issue(&self, user_id: Uuid, email: &str) -> Result<(), DomainError> {
        let code = self.generator.generate();

        // Store hashed in Redis with TTL. A new `store` call overwrites any
        // previous code for this user — exactly one active code at a time.
        self.store
            .store(user_id, &code, DEFAULT_CODE_TTL_SECONDS)
            .await?;

        // Surfaced, not swallowed: the caller must be able to tell the user
        // "delivery failed, try again" instead of waiting for a code that
        // was never sent.
        self.sender.send_login_code(email, &code).await?;

        Ok(())
    }

    /// Verifies a code and consumes it on success (one-time use).
    ///
    /// Wrong, expired and absent codes are indistinguishable on purpose: the
    /// store cannot tell them apart, and exposing the difference would only
    /// help an attacker enumerate live codes.
    pub async fn verify(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError> {
        self.store.verify_and_consume(user_id, code).await
    }
}
