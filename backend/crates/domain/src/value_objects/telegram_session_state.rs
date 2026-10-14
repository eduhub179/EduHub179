//! Telegram authorization session state machine.
//!
//! States:
//! - `Unauthenticated`: no session exists.
//! - `AwaitingEmail`: user sent /start, waiting for school email.
//! - `AwaitingCode`: email accepted, code generated in Redis, waiting for code input.
//! - `Authenticated`: code verified, binding active.
//!
//! Dependencies: Only `crate::errors::DomainError`.
//! Guarantees: Invalid state strings from Redis return `Err`, preventing garbage.
use crate::errors::DomainError;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramSessionState {
    Unauthenticated,
    AwaitingEmail,
    AwaitingCode,
    Authenticated,
}

impl FromStr for TelegramSessionState {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unauthenticated" => Ok(Self::Unauthenticated),
            "awaiting_email" => Ok(Self::AwaitingEmail),
            "awaiting_code" => Ok(Self::AwaitingCode),
            "authenticated" => Ok(Self::Authenticated),
            _ => Err(DomainError::InternalError),
        }
    }
}

impl std::fmt::Display for TelegramSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Unauthenticated => "unauthenticated",
            Self::AwaitingEmail => "awaiting_email",
            Self::AwaitingCode => "awaiting_code",
            Self::Authenticated => "authenticated",
        };
        write!(f, "{}", s)
    }
}
