//! FILE: state.rs
//! PURPOSE: Shared state for Telegram bot handlers.
//!          Holds the teloxide Bot instance and application use cases.
//! DEPENDENCIES: `teloxide`, `application`.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use crate::agent_forward::AgentForwarder;
use application::use_cases::telegram::TelegramAuthUseCase;
use domain::repositories::telegram_bindigs_repository::TelegramBindingRepository;
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
    /// "Is this Telegram account logged in?" — plain DM text from a logged-in user goes to the agent.
    pub bindings: Arc<dyn TelegramBindingRepository>,
    /// Chat agent endpoint; `None` = the bot works alone, as before.
    pub agent: Option<Arc<AgentForwarder>>,
}
