//! Sign-in by school email.
//!
//! Flow:
//! 1. `start(email)` — does the account have a password?
//!    - yes → the person enters it, `login(email, password)`;
//!    - no (first login) → a 6-digit code is emailed.
//! 2. `verify_code(email, code)` → a one-time setup token (15 min). The code alone does
//!    not log in: a password is mandatory.
//! 3. `complete_first_login(email, setup_token, password)` → the password is saved and the
//!    person is logged in. From then on only the password works; codes are refused.
//!
//! Accounts are created by the school (admin panel / import), never here: an email
//! without an account is `UserNotFound`.
//!
//! Brute force: a code burns after 5 wrong entries (`AuthCodeStore`), a new one can be
//! sent once a minute, the setup token is single-use and random (not guessable).
//!
//! Dependencies: `domain` (ports, repositories, `Login`, password rule).
//! Guarantees: all methods return `Result`, no panics; raw codes, tokens and passwords
//! are never stored or logged here.

use std::sync::Arc;

use domain::{
    entities::user::User,
    errors::DomainError,
    ports::{
        auth::{AuthCodeStore, CredentialsStore},
        session_security::SessionTokenGenerator,
    },
    repositories::user_repository::UserRepository,
    value_objects::{login::Login, password::validate_new_password},
};

use super::OneTimeCodeService;

/// A new code can be emailed at most once per this many seconds.
pub const CODE_RESEND_COOLDOWN_SECONDS: u64 = 60;
/// How long the person has to choose a password after entering the code.
pub const SETUP_TOKEN_TTL_SECONDS: u64 = 15 * 60;

/// What the sign-in form asks for next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    /// The account has a password.
    Password,
    /// First login: a code was emailed (or was emailed less than a minute ago).
    Code,
}

/// Result of `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginStart {
    pub method: LoginMethod,
    /// False if a code had been sent less than `CODE_RESEND_COOLDOWN_SECONDS` ago and
    /// that one is still to be used (no second email).
    pub code_sent: bool,
}

pub struct EmailLoginService {
    users: Arc<dyn UserRepository>,
    credentials: Arc<dyn CredentialsStore>,
    codes: Arc<OneTimeCodeService>,
    /// One-time setup tokens (another key prefix than login codes).
    setup_tokens: Arc<dyn AuthCodeStore>,
    token_generator: Arc<dyn SessionTokenGenerator>,
}

impl EmailLoginService {
    pub fn new(
        users: Arc<dyn UserRepository>,
        credentials: Arc<dyn CredentialsStore>,
        codes: Arc<OneTimeCodeService>,
        setup_tokens: Arc<dyn AuthCodeStore>,
        token_generator: Arc<dyn SessionTokenGenerator>,
    ) -> Self {
        Self {
            users,
            credentials,
            codes,
            setup_tokens,
            token_generator,
        }
    }

    /// First step of the form: password or emailed code.
    ///
    /// # Fail-safe поведение
    /// `InvalidEmailFormat` for an address outside the school domain, `UserNotFound`
    /// for an unknown one, `UserIsInactive` for a deactivated account.
    pub async fn start(&self, email: &str) -> Result<LoginStart, DomainError> {
        let (user, email) = self.find_user(email).await?;
        if self.credentials.has_password(user.id).await? {
            return Ok(LoginStart { method: LoginMethod::Password, code_sent: false });
        }
        let code_sent = match self
            .codes
            .issue_with_cooldown(user.id, &email, CODE_RESEND_COOLDOWN_SECONDS)
            .await
        {
            Ok(()) => true,
            // A code is already on its way: the form simply asks for it.
            Err(DomainError::AuthCodeCooldown) => false,
            Err(error) => return Err(error),
        };
        Ok(LoginStart { method: LoginMethod::Code, code_sent })
    }

    /// "Send again" on the code step.
    ///
    /// # Fail-safe поведение
    /// `PasswordAlreadySet` once the first login is over; `AuthCodeCooldown` within a
    /// minute of the previous code.
    pub async fn resend_code(&self, email: &str) -> Result<(), DomainError> {
        let (user, email) = self.find_user(email).await?;
        self.require_no_password(&user).await?;
        self.codes
            .issue_with_cooldown(user.id, &email, CODE_RESEND_COOLDOWN_SECONDS)
            .await
    }

    /// Checks the emailed code; returns a one-time token for `complete_first_login`.
    ///
    /// # Fail-safe поведение
    /// `InvalidAuthCode` for a wrong, expired or burned code (5 wrong entries);
    /// `PasswordAlreadySet` once the first login is over.
    pub async fn verify_code(&self, email: &str, code: &str) -> Result<String, DomainError> {
        let (user, _) = self.find_user(email).await?;
        self.require_no_password(&user).await?;
        if !self.codes.verify(user.id, code.trim()).await? {
            return Err(DomainError::InvalidAuthCode);
        }
        let token = self.token_generator.generate_session_token();
        self.setup_tokens
            .store(user.id, &token, SETUP_TOKEN_TTL_SECONDS)
            .await?;
        Ok(token)
    }

    /// Saves the first password and returns the user to log in.
    ///
    /// # Fail-safe поведение
    /// The password is checked first (`InvalidPasswordFormat`), so a weak one does not
    /// burn the token. `InvalidAuthCode` for a wrong, expired or used token;
    /// `PasswordAlreadySet` if the password was set meanwhile (e.g. in another tab).
    pub async fn complete_first_login(
        &self,
        email: &str,
        setup_token: &str,
        password: &str,
    ) -> Result<User, DomainError> {
        validate_new_password(password)?;
        let (user, _) = self.find_user(email).await?;
        self.require_no_password(&user).await?;
        if !self.setup_tokens.verify_and_consume(user.id, setup_token.trim()).await? {
            return Err(DomainError::InvalidAuthCode);
        }
        self.credentials.set_password(user.id, password).await?;
        Ok(user)
    }

    /// Sign-in with email and password.
    ///
    /// # Fail-safe поведение
    /// `InvalidCredentials` for an unknown email or a wrong password (the same answer);
    /// `PasswordNotSet` if the first login is not done yet (the form switches to the
    /// code step); `UserIsInactive` for a deactivated account.
    pub async fn login(&self, email: &str, password: &str) -> Result<User, DomainError> {
        let (user, _) = match self.find_user(email).await {
            Ok(found) => found,
            Err(DomainError::UserNotFound) => return Err(DomainError::InvalidCredentials),
            Err(error) => return Err(error),
        };
        if !self.credentials.has_password(user.id).await? {
            return Err(DomainError::PasswordNotSet);
        }
        if !self.credentials.verify_password(user.id, password).await? {
            return Err(DomainError::InvalidCredentials);
        }
        Ok(user)
    }

    /// The account of a school email and the normalized address.
    async fn find_user(&self, email: &str) -> Result<(User, String), DomainError> {
        let login = Login::from_email(email)?;
        let user = self.users.get_by_login(login.as_str()).await?;
        if !user.is_active {
            return Err(DomainError::UserIsInactive);
        }
        Ok((user, login.email()))
    }

    async fn require_no_password(&self, user: &User) -> Result<(), DomainError> {
        if self.credentials.has_password(user.id).await? {
            return Err(DomainError::PasswordAlreadySet);
        }
        Ok(())
    }
}
