//! FILE: help.rs
//! PURPOSE: Handlers for `/help` and unknown commands.
//! DEPENDENCIES: `teloxide`, `crate::bot`.
//! MASTER-DOC: docs/CONTRIBUTING.en.md

use crate::bot::HandlerError;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

use super::Command;

/// Handles the `/help` command.
///
/// Uses `Command::descriptions()` to auto-generate the list of commands
/// from the `BotCommands` enum — adding a new command automatically updates
/// the help text without touching this handler.
pub async fn handle_help(bot: Bot, msg: Message) -> Result<(), HandlerError> {
    let help_text = format!(
        "🤖 *EduHub179 Bot*\n\n\
        {}\n\n",
        Command::descriptions()
    );

    bot.send_message(msg.chat.id, help_text)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    Ok(())
}

/// Handles an unrecognized `/something` command.
///
/// Fail-safe: does not panic, just informs the user and points to `/help`.
/// The raw text is intentionally NOT logged to avoid leaking user input.
pub async fn handle_unknown_command(
    bot: Bot,
    msg: Message,
    raw_text: &str,
) -> Result<(), HandlerError> {
    // Extract the command token (e.g. "/foo" from "/foo bar") for the message.
    let cmd_token = raw_text.split_whitespace().next().unwrap_or(raw_text);

    let reply = format!(
        "⚠️ Неизвестная команда: `{}`\\.\n\
Используйте /help для списка доступных команд\\.",
        cmd_token
    );

    bot.send_message(msg.chat.id, reply)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    Ok(())
}
