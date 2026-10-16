//! FILE: state.rs
//! PURPOSE: Shared state for Telegram bot handlers.
//!          Holds the teloxide Bot instance and application use cases.
//! DEPENDENCIES: `teloxide`, `application`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use application::use_cases::telegram::TelegramAuthUseCase;
use std::sync::Arc;
use teloxide::Bot;

/// Shared state injected into every handler via `dptree::deps!`.
///
/// Cloning this struct is cheap (Arcs + teloxide Bot which is internally Arc'd).
#[derive(Clone)]
pub struct BotState {
    /// The teloxide bot instance used to send messages.
    pub bot: Bot,
    /// Unified Telegram authorization flow.
    pub auth_use_case: Arc<TelegramAuthUseCase>,
    // Future: pub schedule_service: Arc<ScheduleService>,
    pub bot_username: String,
}
