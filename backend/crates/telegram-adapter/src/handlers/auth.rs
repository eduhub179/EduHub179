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
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

/// `callback_data` of the "send the code again" button.
pub const RESEND_CALLBACK: &str = "auth:resend";

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
    respond_with_result(&bot, msg.chat.id, result).await
}

/// `/resend` command: email a new code without typing the email again.
pub async fn handle_resend(bot: Bot, state: BotState, msg: Message) -> Result<(), HandlerError> {
    if !msg.chat.is_private() {
        return Ok(());
    }
    let Some(telegram_id) = sender_id(&msg) else { return Ok(()) };
    run_auth_step(&bot, &state, msg.chat.id, telegram_id, TelegramAuthAction::ResendCode).await
}

/// Press on the "send the code again" button under the bot's messages.
pub async fn handle_resend_button(
    bot: Bot,
    state: BotState,
    q: CallbackQuery,
) -> Result<(), HandlerError> {
    // Stop the button's spinner first, whatever happens next.
    bot.answer_callback_query(q.id.clone()).await?;
    let Ok(telegram_id) = i64::try_from(q.from.id.0) else { return Ok(()) };
    // The button only appears in the private chat, whose id is the person's id.
    let chat_id = ChatId(telegram_id);
    run_auth_step(&bot, &state, chat_id, telegram_id, TelegramAuthAction::ResendCode).await
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

    // "123 456" or "123-456" is a code too: people copy it from the email with spaces.
    let digits: String = text.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    let action = if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit()) {
        TelegramAuthAction::VerifyCode { code: digits }
    } else {
        TelegramAuthAction::SubmitEmail { email: text }
    };

    run_auth_step(&bot, &state, msg.chat.id, telegram_id, action).await
}

/// Runs one auth step and answers in `chat_id`.
async fn run_auth_step(
    bot: &Bot,
    state: &BotState,
    chat_id: ChatId,
    telegram_id: i64,
    action: TelegramAuthAction,
) -> Result<(), HandlerError> {
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
            bot.send_message(chat_id, reply).await?;
            return Ok(());
        }
        Err(DomainError::InvalidEmailFormat) => {
            let reply = "❌ Неверный формат почты.\nПожалуйста, введите полную школьную почту (например, s27b_ivanov@179.ru).";
            bot.send_message(chat_id, reply).await?;
            return Ok(());
        }
        Err(DomainError::UserIsInactive) => {
            let reply =
                "❌ Ваш аккаунт деактивирован.\nПожалуйста, обратитесь к администратору школы.";
            bot.send_message(chat_id, reply).await?;
            return Ok(());
        }
        Err(DomainError::InternalError) => {
            let reply = "⚠️ Не удалось отправить код на почту.\n\
                         Это может быть временная проблема. Пожалуйста:\n\
                         • Подождите минуту и попробуйте снова (/start)\n\
                         • Если проблема повторяется — напишите администратору";
            bot.send_message(chat_id, reply).await?;
            return Ok(());
        }
        // Unexpected domain errors (DB down, Redis down) bubble up to HandlerError
        Err(e) => {
            tracing::error!(error = ?e, "Unexpected domain error in auth flow");
            let reply = "⚠️ Произошла непредвиденная ошибка. Пожалуйста, попробуйте через минуту.";
            bot.send_message(chat_id, reply).await?;
            return Ok(());
        }
    };

    respond_with_result(bot, chat_id, result).await
}

/// Telegram id of the person who sent the message.
fn sender_id(msg: &Message) -> Option<i64> {
    msg.from().and_then(|u| i64::try_from(u.id.0).ok())
}

/// Maps the domain's `TelegramAuthResult` to a user-facing Telegram message.
///
/// Keeps the UI text in one place, separated from the routing logic.
/// Messages on the code step carry the "send the code again" button.
async fn respond_with_result(
    bot: &Bot,
    chat_id: ChatId,
    result: TelegramAuthResult,
) -> Result<(), HandlerError> {
    let (text, resend_button) = match result {
        TelegramAuthResult::AlreadyAuthenticated { .. } => (
            "✅ Вы уже авторизованы.\nИспользуйте /help для списка доступных команд.".to_string(),
            false,
        ),
        TelegramAuthResult::AwaitingEmail => (
            "👋 Добро пожаловать в EduHub179!\nПожалуйста, введите вашу школьную почту (например, s27b_ivanov@179.ru).".to_string(),
            false,
        ),
        TelegramAuthResult::CodeGenerated => (
            "📧 Код отправлен на вашу школьную почту.\nПожалуйста, введите 6-значный код из письма.\n\nПисьмо не пришло? Проверьте «Спам» или нажмите кнопку ниже.".to_string(),
            true,
        ),
        TelegramAuthResult::Authenticated { .. } => (
            "🎉 Авторизация успешна!\nТеперь вам доступны команды бота. Используйте /help.".to_string(),
            false,
        ),
        TelegramAuthResult::InvalidState => (
            "⚠️ Время на вход истекло или вход ещё не начат.\nОтправьте /start, чтобы войти.".to_string(),
            false,
        ),
        TelegramAuthResult::WrongCode { attempts_left } => {
            let left = match attempts_left {
                Some(n) => format!("Осталось попыток: {n}.\n"),
                None => String::new(),
            };
            (
                format!("❌ Неверный код. Проверьте его в письме и введите ещё раз.\n{left}\nОшиблись в почте? Просто отправьте правильную почту."),
                true,
            )
        }
        TelegramAuthResult::CodeExpired => (
            "⌛ Этот код больше не действует: истёк срок или было слишком много неверных попыток.\nНажмите «Отправить код заново», и придёт новый код на ту же почту.\nДругая почта — отправьте /start.".to_string(),
            true,
        ),
        TelegramAuthResult::NotACode => (
            "🔢 Жду 6-значный код из письма (только цифры).\nОшиблись в почте? Отправьте правильную почту целиком.".to_string(),
            true,
        ),
        TelegramAuthResult::CodeResent => (
            "📧 Отправил новый код на вашу школьную почту. Старый код больше не действует.\nВведите 6-значный код из нового письма.".to_string(),
            true,
        ),
        TelegramAuthResult::ResendTooSoon => (
            "⏳ Код уже отправлен меньше минуты назад — введите код из последнего письма.\nЕсли письма нет, подождите минуту и нажмите «Отправить код заново».".to_string(),
            true,
        ),
    };

    let request = bot.send_message(chat_id, text);
    if resend_button {
        let keyboard = InlineKeyboardMarkup::new([[InlineKeyboardButton::callback(
            "🔁 Отправить код заново",
            RESEND_CALLBACK,
        )]]);
        request.reply_markup(keyboard).await?;
    } else {
        request.await?;
    }
    Ok(())
}
