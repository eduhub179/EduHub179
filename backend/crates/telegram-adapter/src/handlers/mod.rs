//! FILE: mod.rs
//! PURPOSE: Top-level message router using teloxide's built-in command parser.
//! DEPENDENCIES: `teloxide`, `crate::state`, `crate::bot`, `crate::handlers::*`.
//! MASTER-DOC: docs/CONTRIBUTING.md

pub mod auth;
pub mod fuzzy;
pub mod help;
pub mod schedule;
pub mod view;

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
    #[command(description = "отправить код входа на почту заново")]
    Resend,
    #[command(description = "показать справку")]
    Help,
    #[command(description = "расписание с заменами, можно добавить: завтра, неделя")]
    Schedule(String),
    #[command(description = "расписание на завтра")]
    Tomorrow,
    #[command(description = "расписание на неделю")]
    Week,
    #[command(description = "домашнее задание на завтра, можно добавить: неделя")]
    Homework(String),
}

pub async fn message_handler(bot: Bot, state: BotState, msg: Message) -> Result<(), HandlerError> {
    let text_opt = msg.text().map(|t| t.to_string());

    if let Some(text) = text_opt {
        // 1. Try to parse as a known command
        if let Ok(cmd) = Command::parse(&text, &state.bot_username) {
            return run_command(bot, state, msg, cmd).await;
        }

        // 2. Looks like a command but wasn't parsed: a typo («/shedule», «/homewrok», Russian
        //    layout «/ысрувгду») → the closest command; nothing close → unknown
        if text.starts_with('/') {
            if let Some(cmd) =
                fuzzy::fix_command(&text).and_then(|fixed| Command::parse(&fixed, &state.bot_username).ok())
            {
                return run_command(bot, state, msg, cmd).await;
            }
            return help::handle_unknown_command(bot, msg, &text).await;
        }

        // 3. Plain text → treat as auth payload, but only in a private chat: in a class
        //    chat it is people talking, not an email or a code.
        if msg.chat.is_private() {
            return auth::handle_text_input(bot, state, msg, text).await;
        }
    }

    Ok(())
}

async fn run_command(bot: Bot, state: BotState, msg: Message, cmd: Command) -> Result<(), HandlerError> {
    match cmd {
        Command::Start => auth::handle_start(bot, state, msg).await,
        Command::Resend => auth::handle_resend(bot, state, msg).await,
        Command::Help => help::handle_help(bot, msg).await,
        Command::Schedule(arg) => schedule::handle_schedule(bot, state, msg, arg).await,
        Command::Tomorrow => schedule::handle_schedule(bot, state, msg, "завтра".to_string()).await,
        Command::Week => schedule::handle_schedule(bot, state, msg, "неделя".to_string()).await,
        Command::Homework(arg) => schedule::handle_homework(bot, state, msg, arg).await,
    }
}
