//! FILE: schedule.rs
//! PURPOSE: Stub handler for schedule commands.
//! DEPENDENCIES: `teloxide`, `crate::bot`.

use crate::bot::HandlerError;
use teloxide::prelude::*;

/// Placeholder for the /schedule command.
pub async fn handle_schedule(bot: Bot, msg: Message) -> Result<(), HandlerError> {
    bot.send_message(
        msg.chat.id,
        "📅 Функция просмотра расписания пока в разработке.",
    )
    .await?;
    Ok(())
}
