//! FILE: auth.rs
//! PURPOSE: Telegram handlers for the authorization flow (/start, email input, code input).
//! DEPENDENCIES: `teloxide`, `application`, `domain`, `crate::state`, `crate::bot`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use crate::bot::HandlerError;
use crate::state::BotState;
use application::use_cases::telegram::{
    TelegramAuthAction, TelegramAuthCommand, TelegramAuthResult,
};
use domain::errors::DomainError;
use teloxide::prelude::*;

/// Handles the `/start` command.
///
/// Resets or initializes the auth session. If the user is already bound, the use case will return `AlreadyAuthenticated`.
pub async fn handle_start(bot: Bot, state: BotState, msg: Message) -> Result<(), HandlerError> {
    // Login only in a private chat: in a group the email and code would be seen by everyone.
    if !msg.chat.is_private() {
        bot.send_message(msg.chat.id, "🔒 Чтобы войти, напишите мне в личку /start.")
            .await?;
        return Ok(());
    }
    // The binding is for the person (`from.id`), not the chat: in a private chat they are
    // equal, so existing bindings stay valid; the agent and /schedule look up `from.id` too.
    let Some(telegram_id) = sender_id(&msg) else { return Ok(()) };

    let cmd = TelegramAuthCommand {
        telegram_id,
        action: TelegramAuthAction::Start,
    };

    let result = state.auth_use_case.execute(cmd).await?;
    respond_with_result(bot, msg, result).await
}

/// Handles arbitrary text input (expected to be an email or a 6-digit code).
///
/// Heuristic routing: If the text is exactly 6 ASCII digits, we treat it as
/// a verification code. Otherwise, we treat it as an email.
/// The use case's state machine will reject it if the session doesn't match.
pub async fn handle_text_input(
    bot: Bot,
    state: BotState,
    msg: Message,
    text: String,
) -> Result<(), HandlerError> {
    if !msg.chat.is_private() {
        return Ok(());
    }
    let Some(telegram_id) = sender_id(&msg) else { return Ok(()) };

    let action = if text.len() == 6 && text.chars().all(|c| c.is_ascii_digit()) {
        TelegramAuthAction::VerifyCode { code: text }
    } else {
        TelegramAuthAction::SubmitEmail { email: text }
    };

    let cmd = TelegramAuthCommand {
        telegram_id,
        action,
    };

    // Execute the use case. We intercept specific DomainErrors here to provide
    // user-friendly feedback instead of letting them bubble up as 500s/logs.
    let result = match state.auth_use_case.execute(cmd).await {
        Ok(res) => res,
        Err(DomainError::UserNotFound) => {
            let reply = "❌ Пользователь с такой почтой не найден в системе.\nУбедитесь, что вы используете школьную почту (например, s27b_ivanov@179.ru).";
            bot.send_message(msg.chat.id, reply).await?;
            return Ok(());
        }
        Err(DomainError::InvalidEmailFormat) => {
            let reply = "❌ Неверный формат почты.\nПожалуйста, введите полную школьную почту (например, s27b_ivanov@179.ru).";
            bot.send_message(msg.chat.id, reply).await?;
            return Ok(());
        }
        Err(DomainError::UserIsInactive) => {
            let reply =
                "❌ Ваш аккаунт деактивирован.\nПожалуйста, обратитесь к администратору школы.";
            bot.send_message(msg.chat.id, reply).await?;
            return Ok(());
        }
        Err(DomainError::InternalError) => {
            let reply = "⚠️ Не удалось отправить код на почту.\n\
                         Это может быть временная проблема. Пожалуйста:\n\
                         • Подождите минуту и попробуйте снова (/start)\n\
                         • Если проблема повторяется — напишите администратору";
            bot.send_message(msg.chat.id, reply).await?;
            return Ok(());
        }
        // Unexpected domain errors (DB down, Redis down) bubble up to HandlerError
        Err(e) => {
            tracing::error!(error = ?e, "Unexpected domain error in auth flow");
            let reply = "⚠️ Произошла непредвиденная ошибка. Пожалуйста, попробуйте через минуту.";
            bot.send_message(msg.chat.id, reply).await?;
            return Ok(());
        }
    };

    respond_with_result(bot, msg, result).await
}

/// Telegram id of the person who sent the message.
fn sender_id(msg: &Message) -> Option<i64> {
    msg.from().and_then(|u| i64::try_from(u.id.0).ok())
}

/// Maps the domain's `TelegramAuthResult` to a user-facing Telegram message.
///
/// Keeps the UI text in one place, separated from the routing logic.
async fn respond_with_result(
    bot: Bot,
    msg: Message,
    result: TelegramAuthResult,
) -> Result<(), HandlerError> {
    let text = match result {
        TelegramAuthResult::AlreadyAuthenticated { .. } => {
            "✅ Вы уже авторизованы.\nИспользуйте /help для списка доступных команд.".to_string()
        }
        TelegramAuthResult::AwaitingEmail => {
            "👋 Добро пожаловать в EduHub179!\nПожалуйста, введите вашу школьную почту (например, s27b_ivanov@179.ru).".to_string()
        }
        TelegramAuthResult::CodeGenerated => {
            "📧 Код отправлен на вашу школьную почту.\nПожалуйста, введите 6-значный код из письма.".to_string()
        }
        TelegramAuthResult::Authenticated { .. } => {
            "🎉 Авторизация успешна!\nТеперь вам доступны команды бота. Используйте /help.".to_string()
        }
        TelegramAuthResult::InvalidState => {
            "⚠️ Неверное состояние сессии.\nПожалуйста, отправьте /start, чтобы начать процесс заново.".to_string()
        }
        TelegramAuthResult::CodeExpired => {
            "❌ Неверный или истекший код.\nПожалуйста, отправьте /start, чтобы запросить новый код.".to_string()
        }
    };

    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}
