//! FILE: homework.rs
//! PURPOSE: Stub handler for homework commands.
//! DEPENDENCIES: `teloxide`, `crate::bot`.

use crate::bot::HandlerError;
use teloxide::prelude::*;

/// Placeholder for the /homework command.
pub async fn handle_homework(bot: Bot, msg: Message) -> Result<(), HandlerError> {
    bot.send_message(
        msg.chat.id,
        "📚 Функция просмотра домашних заданий пока в разработке.",
    )
    .await?;
    Ok(())
}
