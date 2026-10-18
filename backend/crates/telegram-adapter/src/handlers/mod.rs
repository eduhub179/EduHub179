//! FILE: mod.rs
//! PURPOSE: Top-level message router using teloxide's built-in command parser.
//! DEPENDENCIES: `teloxide`, `crate::state`, `crate::bot`, `crate::handlers::*`.
//! MASTER-DOC: docs/CONTRIBUTING.md

pub mod auth;
pub mod help;
pub mod homework;
pub mod schedule;

use crate::bot::HandlerError;
use crate::state::BotState;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

/// Supported bot commands.
///
/// The `BotCommands` macro automatically handles:
/// - Case-insensitivity (/start, /START)
/// - Bot mentions (/start@EduHub179Bot)
/// - Argument parsing (if needed in the future)
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Доступные команды:")]
pub enum Command {
    #[command(description = "начать авторизацию или сбросить сессию")]
    Start,
    #[command(description = "показать справку")]
    Help,
    #[command(description = "посмотреть расписание")]
    Schedule,
    #[command(description = "посмотреть домашние задания")]
    Homework,
}

pub async fn message_handler(bot: Bot, state: BotState, msg: Message) -> Result<(), HandlerError> {
    let text_opt = msg.text().map(|t| t.to_string());

    if let Some(text) = text_opt {
        // 1. Try to parse as a known command
        if let Ok(cmd) = Command::parse(&text, &state.bot_username) {
            return match cmd {
                Command::Start => auth::handle_start(bot, state, msg).await,
                Command::Help => help::handle_help(bot, msg).await,
                Command::Schedule => schedule::handle_schedule(bot, msg).await,
                Command::Homework => homework::handle_homework(bot, msg).await,
            };
        }

        // 2. Looks like a command but wasn't parsed → unknown
        if text.starts_with('/') {
            return help::handle_unknown_command(bot, msg, &text).await;
        }

        // 3. Plain text → treat as auth payload
        return auth::handle_text_input(bot, state, msg, text).await;
    }

    Ok(())
}
