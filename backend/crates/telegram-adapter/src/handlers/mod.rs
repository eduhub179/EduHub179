//! FILE: mod.rs
//! PURPOSE: Top-level message router. Directs incoming updates to specific
//!          feature handlers (auth, schedule, homework).
//! DEPENDENCIES: `teloxide`, `crate::state`, `crate::bot`.

pub mod auth;
pub mod homework;
pub mod schedule;

use crate::bot::HandlerError;
use crate::state::BotState;
use teloxide::prelude::*;

/// Unified entry point for all incoming text messages.
///
/// Routes `/start` to the auth flow. All other text is treated as potential
/// input for the current auth step (email or code).
///
/// Fail-safe: Ignores non-text messages (photos, voice, etc.) silently,
/// as the MVP bot only supports text-based auth and commands.
pub async fn message_handler(bot: Bot, state: BotState, msg: Message) -> Result<(), HandlerError> {
    let text_opt = msg.text().map(|t| t.to_string());

    if let Some(text) = text_opt {
        if text.starts_with("/start") {
            return auth::handle_start(bot, state, msg).await;
        }

        // Any other text is treated as input for the active auth session
        return auth::handle_text_input(bot, state, msg, text).await;
    }

    // Silently ignore non-text messages (photos, stickers, etc.)
    Ok(())
}
