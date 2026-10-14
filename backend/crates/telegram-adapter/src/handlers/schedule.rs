//! FILE: schedule.rs
//! PURPOSE: Stub handler for schedule commands.
//! DEPENDENCIES: `teloxide`, `crate::bot`.

use teloxide::prelude::*;
use crate::bot::HandlerError;

/// Placeholder for the /schedule command.
pub async fn schedule_handler(bot: Bot, msg: Message) -> Result<(), HandlerError> {
    bot.send_message(msg.chat.id, "📅 Функция просмотра расписания пока в разработке.").await?;
    Ok(())
}
