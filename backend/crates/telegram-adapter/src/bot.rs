//! FILE: bot.rs
//! PURPOSE: Entry point for the Telegram bot. Initializes the teloxide Dispatcher,
//!          registers update handlers, and defines the adapter-level error type.
//! DEPENDENCIES: `teloxide`, `domain`, `crate::state`, `crate::handlers`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use teloxide::{
    dispatching::{Dispatcher, UpdateHandler},
    prelude::*,
    types::{Update, UpdateKind},
};
use thiserror::Error;

use crate::handlers;
use crate::state::BotState;
use domain::errors::DomainError;

/// Adapter-level error.
///
/// Separates domain business errors (which are expected and handled gracefully
/// in the handler) from Telegram API network errors (which are unexpected and
/// usually indicate a transient infrastructure issue).
#[derive(Error, Debug)]
pub enum HandlerError {
    /// A business logic error leaked from the use case.
    /// In a fully fail-safe handler, most of these are caught and mapped to
    /// user-friendly messages; this variant is the catch-all for unhandled ones.
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    /// Telegram API request failed (e.g., network timeout, rate limit).
    #[error("Telegram API error: {0}")]
    Telegram(#[from] teloxide::RequestError),
}

/// Builds the update routing schema for the Dispatcher.
///
/// Updates for the chat agent (see `for_agent`) are forwarded to it; everything else goes to the
/// unified `message_handler`, which internally dispatches based on command/text content.
fn schema() -> UpdateHandler<HandlerError> {
    dptree::entry()
        .branch(dptree::filter_async(for_agent).endpoint(forward_to_agent))
        .branch(Update::filter_message().endpoint(handlers::message_handler))
}

/// What the agent handles instead of this bot (only when `AGENT_URL` is set):
/// - messages in class chats (groups): the agent collects homework and lesson changes there;
/// - presses on the agent's buttons (`callback_data` starting with `dm:`);
/// - non-command DMs from a logged-in user: that is homework sent to the bot, recorded with the
///   user as author. DMs from users who are not logged in stay here: they are the email / code input.
async fn for_agent(update: Update, state: BotState) -> bool {
    if state.agent.is_none() {
        return false;
    }
    let msg = match &update.kind {
        UpdateKind::CallbackQuery(q) => return q.data.as_deref().is_some_and(|d| d.starts_with("dm:")),
        UpdateKind::Message(m) | UpdateKind::EditedMessage(m) => m,
        _ => return false,
    };
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        return true;
    }
    if !msg.chat.is_private() || msg.text().is_some_and(|t| t.starts_with('/')) {
        return false;
    }
    let Some(from) = msg.from() else { return false };
    let Ok(telegram_id) = i64::try_from(from.id.0) else { return false };
    state.bindings.get_active_user_by_telegram_id(telegram_id).await.is_ok()
}

async fn forward_to_agent(bot: Bot, update: Update, state: BotState) -> Result<(), HandlerError> {
    let Some(agent) = state.agent.as_ref() else { return Ok(()) };
    if agent.forward(&update).await {
        return Ok(());
    }
    // Agent is down: tell the person in DM; in group chats stay silent.
    if let UpdateKind::Message(m) = &update.kind {
        if m.chat.is_private() {
            bot.send_message(m.chat.id, "⚠️ Приём домашки сейчас недоступен, попробуйте чуть позже.")
                .await?;
        }
    }
    Ok(())
}

/// Starts the Telegram bot polling loop.
///
/// This function blocks until the bot is stopped (e.g., via Ctrl+C, handled
/// internally by teloxide's `enable_ctrlc_handler`).
pub async fn run_bot(state: BotState) {
    let bot = state.bot.clone();

    // Inject BotState into the dependency map so handlers can access use cases.
    let mut dispatcher = Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build();

    dispatcher.dispatch().await;
}
