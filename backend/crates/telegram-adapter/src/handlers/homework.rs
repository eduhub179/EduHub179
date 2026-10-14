//! FILE: homework.rs
//! PURPOSE: Stub handler for homework commands.
//! DEPENDENCIES: `teloxide`, `crate::bot`.

use teloxide::prelude::*;
use crate::bot::HandlerError;

/// Placeholder for the /homework command.
pub async fn homework_handler(bot: Bot, msg: Message) -> Result<(), HandlerError> {
    bot.send_message(msg.chat.id, "📚 Функция просмотра домашних заданий пока в разработке.").await?;
    Ok(())
}
