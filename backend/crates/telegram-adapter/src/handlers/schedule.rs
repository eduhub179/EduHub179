//! FILE: schedule.rs
//! PURPOSE: /schedule (today / tomorrow / week) and /homework for logged-in users.
//! DEPENDENCIES: `teloxide`, `application` (ScheduleService), `crate::state`, `super::view`.
//!
//! Who asks is taken from `msg.from` (the Telegram user), never from the chat:
//! in a group chat the chat id is the group's. Students get their class/group
//! schedule, teachers and admins their own lessons.

use crate::bot::HandlerError;
use crate::state::BotState;
use application::use_cases::schedule::schedule::ScheduleDay;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use domain::entities::user::User;
use domain::errors::DomainError;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use super::view;

/// What period a command asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// `/schedule` without arguments: today, or the next school day in the evening.
    Default,
    Today,
    Tomorrow,
    /// Today and the next 6 days.
    Week,
}

impl Period {
    /// Parses the command argument: "завтра", "неделя", "сегодня" (also English words).
    pub fn parse(arg: &str) -> Period {
        let a = arg.trim().to_lowercase();
        if a.is_empty() {
            Period::Default
        } else if a.contains("зав") || a.contains("tom") {
            Period::Tomorrow
        } else if a.contains("нед") || a.contains("week") {
            Period::Week
        } else if a.contains("сег") || a.contains("tod") {
            Period::Today
        } else {
            Period::Default
        }
    }
}

const NOT_LOGGED_IN: &str =
    "🔒 Сначала войдите: напишите мне в личку /start и введите школьную почту.";
const NOT_PUBLISHED: &str = "🗓 Расписание на эти дни ещё не опубликовано.";
const FAILED: &str = "⚠️ Не удалось загрузить расписание. Попробуйте через минуту.";

/// The logged-in user behind the message, or `None` (the reply is already sent).
async fn current_user(bot: &Bot, state: &BotState, msg: &Message) -> Result<Option<User>, HandlerError> {
    let Some(from) = msg.from() else { return Ok(None) };
    let Ok(telegram_id) = i64::try_from(from.id.0) else { return Ok(None) };
    let user_id = match state.bindings.get_active_user_by_telegram_id(telegram_id).await {
        Ok(id) => id,
        Err(_) => {
            bot.send_message(msg.chat.id, NOT_LOGGED_IN).await?;
            return Ok(None);
        }
    };
    match state.users.get_by_id(user_id).await {
        Ok(user) if user.is_active => Ok(Some(user)),
        Ok(_) => {
            bot.send_message(msg.chat.id, "❌ Ваш аккаунт деактивирован. Обратитесь к администратору.")
                .await?;
            Ok(None)
        }
        Err(e) => {
            tracing::error!(error = ?e, "bot: user of a telegram binding not loaded");
            bot.send_message(msg.chat.id, FAILED).await?;
            Ok(None)
        }
    }
}

/// One day for the user: a student's own schedule, a teacher's own lessons.
async fn load_day(state: &BotState, user: &User, date: NaiveDate) -> Result<ScheduleDay, DomainError> {
    if user.role.is_student() {
        state.schedule.day_schedule(user, user.id, date).await
    } else {
        state.schedule.teacher_day_schedule(user.id, date).await
    }
}

/// Days `from..from+count`, Sundays skipped. A day of an unpublished week is left out;
/// `Err` only if every day failed for that reason (`Ok(None)`) or storage failed.
async fn load_days(
    state: &BotState,
    user: &User,
    from: NaiveDate,
    count: i64,
) -> Result<Option<Vec<ScheduleDay>>, DomainError> {
    let mut days = Vec::new();
    for i in 0..count {
        let date = from + Duration::days(i);
        if date.weekday() == Weekday::Sun {
            continue;
        }
        match load_day(state, user, date).await {
            Ok(day) => days.push(day),
            Err(DomainError::ScheduleWeekNotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(if days.is_empty() { None } else { Some(days) })
}

/// `/schedule [сегодня|завтра|неделя]`.
pub async fn handle_schedule(bot: Bot, state: BotState, msg: Message, arg: String) -> Result<(), HandlerError> {
    let Some(user) = current_user(&bot, &state, &msg).await? else { return Ok(()) };
    let now = view::moscow_now();
    let today = now.date();
    let with_target = !user.role.is_student();

    let (from, count) = match Period::parse(&arg) {
        Period::Default => (view::default_schedule_day(now), 1),
        Period::Today => (today, 1),
        Period::Tomorrow => (today + Duration::days(1), 1),
        Period::Week => (today, 7),
    };

    let text = match load_days(&state, &user, from, count).await {
        Ok(Some(days)) => {
            let mut parts: Vec<String> = days.iter().map(|d| view::format_day(d, today, with_target)).collect();
            if count == 1 {
                parts.push("\nЕщё: /schedule завтра · /schedule неделя · /homework".to_string());
            }
            parts.join("\n")
        }
        Ok(None) => NOT_PUBLISHED.to_string(),
        Err(e) => {
            tracing::error!(error = ?e, "bot: schedule not loaded");
            FAILED.to_string()
        }
    };
    send_long(&bot, &msg, text).await
}

/// `/homework [неделя]`: the next school day, or the coming week.
pub async fn handle_homework(bot: Bot, state: BotState, msg: Message, arg: String) -> Result<(), HandlerError> {
    let Some(user) = current_user(&bot, &state, &msg).await? else { return Ok(()) };
    if !user.role.is_student() {
        bot.send_message(
            msg.chat.id,
            "📚 Домашка в боте показывается ученикам. Ваши уроки: /schedule неделя",
        )
        .await?;
        return Ok(());
    }
    let today = view::moscow_now().date();
    let (from, count) = match Period::parse(&arg) {
        Period::Week => (today + Duration::days(1), 7),
        Period::Today => (today, 1),
        _ => (view::next_school_day(today), 1),
    };
    let text = match load_days(&state, &user, from, count).await {
        Ok(Some(days)) => {
            let mut t = view::format_homework(&days, today);
            if count == 1 {
                t.push_str("\n\nНа неделю: /homework неделя");
            }
            t
        }
        Ok(None) => NOT_PUBLISHED.to_string(),
        Err(e) => {
            tracing::error!(error = ?e, "bot: homework not loaded");
            FAILED.to_string()
        }
    };
    send_long(&bot, &msg, text).await
}

/// Sends with `ParseMode::Html` (the texts from `view` are escaped there).
/// Telegram's limit is 4096 characters; a week of a teacher can be longer.
/// Split only between lines: `view` never opens a tag on one line and closes it on another.
async fn send_long(bot: &Bot, msg: &Message, text: String) -> Result<(), HandlerError> {
    const LIMIT: usize = 3800;
    let mut chunk = String::new();
    for line in text.lines() {
        if chunk.chars().count() + line.chars().count() + 1 > LIMIT && !chunk.is_empty() {
            bot.send_message(msg.chat.id, std::mem::take(&mut chunk))
                .parse_mode(ParseMode::Html)
                .await?;
        }
        chunk.push_str(line);
        chunk.push('\n');
    }
    if !chunk.trim().is_empty() {
        bot.send_message(msg.chat.id, chunk).parse_mode(ParseMode::Html).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Period;

    #[test]
    fn parses_arguments() {
        assert_eq!(Period::parse(""), Period::Default);
        assert_eq!(Period::parse(" Завтра "), Period::Tomorrow);
        assert_eq!(Period::parse("неделя"), Period::Week);
        assert_eq!(Period::parse("на неделю"), Period::Week);
        assert_eq!(Period::parse("что-то"), Period::Default);
        assert_eq!(Period::parse("сегодня"), Period::Today);
        assert_eq!(Period::parse("week"), Period::Week);
    }
}
