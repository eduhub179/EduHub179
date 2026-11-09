//! FILE: auth.rs
//! PURPOSE: Unified Telegram authorization use case.
//! DEPENDENCIES: domain (repositories, value objects, errors),
//!               crate::use_cases::auth::OneTimeCodeService.
//! MASTER-DOC: docs/copilot-instructions.md §6.7

use std::sync::Arc;

use domain::errors::DomainError;
use domain::repositories::telegram_bindigs_repository::TelegramBindingRepository;
use domain::repositories::telegram_session_repository::TelegramSessionRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::login::Login;
use domain::value_objects::telegram_session_state::TelegramSessionState;

use domain::ports::auth::CodeCheck;

use crate::use_cases::auth::OneTimeCodeService;
use super::{TelegramAuthAction, TelegramAuthCommand, TelegramAuthResult};

/// How long the bot waits for the next step (email or code), in seconds.
const SESSION_TTL_SECONDS: u64 = 600;
/// A new code can be emailed at most once per this many seconds (same as web login).
pub const RESEND_COOLDOWN_SECONDS: u64 = 60;

/// Unified use case managing the entire Telegram auth lifecycle.
///
/// Code generation, storage, and delivery are delegated to `OneTimeCodeService` —
/// the same shared service used by web login and password recovery. This keeps
/// OTP rules (TTL, hashing, one-active-code, delivery) identical across channels.
pub struct TelegramAuthUseCase {
    users: Arc<dyn UserRepository>,
    bindings: Arc<dyn TelegramBindingRepository>,
    sessions: Arc<dyn TelegramSessionRepository>,
    codes: Arc<OneTimeCodeService>,
}

impl TelegramAuthUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        bindings: Arc<dyn TelegramBindingRepository>,
        sessions: Arc<dyn TelegramSessionRepository>,
        codes: Arc<OneTimeCodeService>,
    ) -> Self {
        Self {
            users,
            bindings,
            sessions,
            codes,
        }
    }

    /// Executes the requested authorization step.
    ///
    /// Fail-safe: Routes to the specific handler based on the action.
    /// Invalid states or expired codes are returned as domain results,
    /// never as panics or thrown exceptions.
    pub async fn execute(
        &self,
        cmd: TelegramAuthCommand,
    ) -> Result<TelegramAuthResult, DomainError> {
        // Extract context once, pass it down to the specific handler.
        let telegram_id = cmd.telegram_id;

        match cmd.action {
            TelegramAuthAction::Start => self.handle_start(telegram_id).await,
            TelegramAuthAction::SubmitEmail { email } => {
                self.handle_submit_email(telegram_id, email).await
            }
            TelegramAuthAction::VerifyCode { code } => {
                self.handle_verify_code(telegram_id, code).await
            }
            TelegramAuthAction::ResendCode => self.handle_resend_code(telegram_id).await,
        }
    }

    /// Step 1: Initialize or reset the auth flow.
    async fn handle_start(&self, telegram_id: i64) -> Result<TelegramAuthResult, DomainError> {
        // Fail-fast: check if already authenticated
        match self.bindings.get_active_user_by_telegram_id(telegram_id).await {
            Ok(user_id) => return Ok(TelegramAuthResult::AlreadyAuthenticated { user_id }),
            Err(DomainError::TelegramBindingNotFound) => {} // Proceed to start
            Err(e) => return Err(e),
        }

        // Reset or initialize session to AwaitingEmail (TTL 600s)
        self.sessions
            .set_state(telegram_id, TelegramSessionState::AwaitingEmail, SESSION_TTL_SECONDS)
            .await?;

        Ok(TelegramAuthResult::AwaitingEmail)
    }

    /// Step 2: Process the submitted email, issue code via shared service,
    /// and store pending binding.
    async fn handle_submit_email(
        &self,
        telegram_id: i64,
        email: String,
    ) -> Result<TelegramAuthResult, DomainError> {
        // 1. Verify session state. On the code step a new email is accepted too (the
        //    person mistyped it); other text there is just not a code.
        let state = self.sessions.get_state(telegram_id).await?;
        match state {
            TelegramSessionState::AwaitingEmail => {}
            TelegramSessionState::AwaitingCode if email.contains('@') => {}
            TelegramSessionState::AwaitingCode => return Ok(TelegramAuthResult::NotACode),
            _ => return Ok(TelegramAuthResult::InvalidState),
        }

        // 2. Parse email to Login (validates format and domain)
        let login = Login::from_email(&email)?;

        // 3. Fetch user by login
        let user = match self.users.get_by_login(login.as_str()).await {
            Ok(u) => u,
            Err(DomainError::UserNotFound) => return Err(DomainError::UserNotFound),
            Err(e) => return Err(e),
        };

        // 4. Check if user is active
        if !user.is_active {
            return Err(DomainError::UserIsInactive);
        }

        // 5. Delegate code lifecycle to the shared service:
        //    generate -> hash -> store in Redis (TTL 600s) -> send to email.
        //    At most one email a minute per account: within the minute the code
        //    already sent stays valid and the person just enters it.
        //    The raw code never reaches this method's return value.
        let result = match self
            .codes
            .issue_with_cooldown(user.id, &login.email(), RESEND_COOLDOWN_SECONDS)
            .await
        {
            Ok(()) => TelegramAuthResult::CodeGenerated,
            Err(DomainError::AuthCodeCooldown) => TelegramAuthResult::ResendTooSoon,
            Err(e) => return Err(e),
        };

        // 6. Store pending binding in PostgreSQL
        self.bindings.store_pending_binding(telegram_id, user.id).await?;

        // 7. Advance Telegram-specific state machine
        self.sessions
            .set_state(telegram_id, TelegramSessionState::AwaitingCode, SESSION_TTL_SECONDS)
            .await?;

        // The bot adapter will catch this and reply:
        // "Код отправлен на вашу школьную почту."
        Ok(result)
    }

    /// Step 3: Verify the code and activate the binding.
    async fn handle_verify_code(
        &self,
        telegram_id: i64,
        code: String,
    ) -> Result<TelegramAuthResult, DomainError> {
        // 1. Verify session state
        let state = self.sessions.get_state(telegram_id).await?;
        if state != TelegramSessionState::AwaitingCode {
            return Ok(TelegramAuthResult::InvalidState);
        }

        // 2. CRITICAL: resolve the pending user_id BEFORE verifying the code.
        //    The code belongs to a specific user; verifying without knowing
        //    whom it belongs to would activate a binding for whoever's code
        //    happened to match.
        let user_id = match self.bindings.get_pending_user_by_telegram_id(telegram_id).await {
            Ok(id) => id,
            Err(DomainError::TelegramBindingNotFound) => return Ok(TelegramAuthResult::InvalidState),
            Err(e) => return Err(e),
        };

        // 3. Delegate verification to the shared service.
        //    It handles hashing comparison and atomic deletion (consume) in Redis.
        //    A wrong code does not end the session: the person types it again until
        //    the store burns the code (5 wrong entries), then asks for a new one.
        match self.codes.check(user_id, &code).await? {
            CodeCheck::Valid => {}
            CodeCheck::Wrong { attempts_left } => {
                return Ok(TelegramAuthResult::WrongCode { attempts_left })
            }
            CodeCheck::Gone => return Ok(TelegramAuthResult::CodeExpired),
        }

        // 4. Activate the binding in PostgreSQL
        let activated_user_id = self.bindings.activate_binding(telegram_id).await?;

        // 5. Clean up Telegram session
        self.sessions.delete(telegram_id).await?;

        Ok(TelegramAuthResult::Authenticated {
            user_id: activated_user_id,
        })
    }

    /// "Send the code again": a new code to the email of the pending binding,
    /// without typing the email once more.
    async fn handle_resend_code(&self, telegram_id: i64) -> Result<TelegramAuthResult, DomainError> {
        let state = self.sessions.get_state(telegram_id).await?;
        if state != TelegramSessionState::AwaitingCode {
            return Ok(TelegramAuthResult::InvalidState);
        }
        let user_id = match self.bindings.get_pending_user_by_telegram_id(telegram_id).await {
            Ok(id) => id,
            Err(DomainError::TelegramBindingNotFound) => return Ok(TelegramAuthResult::InvalidState),
            Err(e) => return Err(e),
        };
        let user = self.users.get_by_id(user_id).await?;
        if !user.is_active {
            return Err(DomainError::UserIsInactive);
        }
        let login = Login::try_new(&user.login)?;
        match self
            .codes
            .issue_with_cooldown(user.id, &login.email(), RESEND_COOLDOWN_SECONDS)
            .await
        {
            Ok(()) => {}
            Err(DomainError::AuthCodeCooldown) => return Ok(TelegramAuthResult::ResendTooSoon),
            Err(e) => return Err(e),
        }
        // A fresh code gets the full time again.
        self.sessions
            .set_state(telegram_id, TelegramSessionState::AwaitingCode, SESSION_TTL_SECONDS)
            .await?;
        Ok(TelegramAuthResult::CodeResent)
    }
}
