//! FILE: bot.rs
//! PURPOSE: Entry point for the Telegram bot. Initializes the teloxide Dispatcher,
//!          registers update handlers, and defines the adapter-level error type.
//! DEPENDENCIES: `teloxide`, `domain`, `crate::state`, `crate::handlers`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use teloxide::{
    dispatching::{Dispatcher, UpdateHandler},
    prelude::*,
    types::Update,
};
use thiserror::Error;

use domain::errors::DomainError;
use crate::handlers;
use crate::state::BotState;

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
/// Currently routes all incoming messages to the unified `message_handler`,
/// which internally dispatches based on command/text content.
fn schema() -> UpdateHandler<HandlerError> {
    dptree::entry()
        .branch(Update::filter_message().endpoint(handlers::message_handler))
}

/// Starts the Telegram bot polling loop.
///
/// This function blocks until the bot is stopped (e.g., via Ctrl+C, handled 
/// internally by teloxide's `enable_ctrlc_handler`).
pub async fn run_bot(state: BotState) {
    let bot = state.bot.clone();
    
    // Inject BotState into the dependency map so handlers can access use cases.
    let dispatcher = Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build();

    tracing::info!("Telegram bot started");
    dispatcher.dispatch().await;
}
