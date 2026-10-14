//! Telegram authorization use case types and result definitions.
//!
//! PURPOSE: Defines the input commands and output results for the Telegram
//!          bot authentication flow.
//! DEPENDENCIES: `domain` (errors), `uuid`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

pub mod auth;

pub use auth::TelegramAuthUseCase;

use uuid::Uuid;

/// The specific action the user wants to perform in the auth flow.
///
/// Contains ONLY the payload (data specific to the step).
/// The identity (`telegram_id`) is provided by the surrounding context
/// (`TelegramAuthCommand`), keeping the intent separate from the transport.
#[derive(Debug, Clone)]
pub enum TelegramAuthAction {
    /// User sent /start or reset command.
    Start,
    /// User provided their school email.
    SubmitEmail { email: String },
    /// User provided the 6-digit code from their email.
    VerifyCode { code: String },
}

/// The complete command passed to the use case.
///
/// Separates the "Who" (context) from the "What" (action).
/// This prevents duplicating `telegram_id` across every action variant
/// and makes it easy to add cross-cutting context later (e.g., trace_id).
#[derive(Debug, Clone)]
pub struct TelegramAuthCommand {
    /// External identifier from the messaging platform (Telegram chat ID).
    pub telegram_id: i64,
    /// The specific authorization step and its payload.
    pub action: TelegramAuthAction,
}

/// Result of a Telegram authorization step, dictating the bot's next UI action.
///
/// SECURITY: `CodeGenerated` does NOT contain the raw code. The code is
/// delivered via `CodeSender` to the user's email. The bot must never echo
/// it back in the chat, as that would defeat the purpose of email verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramAuthResult {
    /// The Telegram ID is already bound to an active user.
    AlreadyAuthenticated { user_id: Uuid },
    /// The bot is waiting for the user to input their school email.
    AwaitingEmail,
    /// The email was accepted and the code was sent to the user's email.
    CodeGenerated,
    /// The code was verified, and the binding is now active.
    Authenticated { user_id: Uuid },
    /// The session state does not match the expected step.
    InvalidState,
    /// The provided code was incorrect or expired.
    CodeExpired,
}
