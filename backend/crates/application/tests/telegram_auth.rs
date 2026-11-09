//! Telegram login by emailed code: a wrong code can be typed again, a new code can be
//! requested without the email, and brute force is still capped.
//!
//! Everything is in memory (no Postgres / Redis): the store mimics `AuthCodeStoreRedis`
//! (5 wrong entries burn the code, one email a minute).
//! Run: `cargo test -p application --test telegram_auth`

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use application::use_cases::auth::OneTimeCodeService;
use application::use_cases::telegram::{
    TelegramAuthAction, TelegramAuthCommand, TelegramAuthResult, TelegramAuthUseCase,
};
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::ports::auth::{AuthCodeStore, CodeCheck, CodeGenerator, CodeSender};
use domain::repositories::telegram_bindigs_repository::TelegramBindingRepository;
use domain::repositories::telegram_session_repository::TelegramSessionRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use domain::value_objects::telegram_session_state::TelegramSessionState;
use uuid::Uuid;

const TG: i64 = 42;
const MAX_WRONG: u32 = 5;

struct Users(User);

#[async_trait::async_trait]
impl UserRepository for Users {
    async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError> {
        if user_id == self.0.id {
            Ok(self.0.clone())
        } else {
            Err(DomainError::UserNotFound)
        }
    }
    async fn get_by_login(&self, login: &str) -> Result<User, DomainError> {
        if login == self.0.login {
            Ok(self.0.clone())
        } else {
            Err(DomainError::UserNotFound)
        }
    }
    async fn get_active_students_by_class(
        &self,
        _class_id: Uuid,
    ) -> Result<Vec<User>, DomainError> {
        Ok(vec![])
    }
    async fn save(&self, user: User) -> Result<User, DomainError> {
        Ok(user)
    }
}

#[derive(Default)]
struct Bindings {
    pending: Mutex<HashMap<i64, Uuid>>,
    active: Mutex<HashMap<i64, Uuid>>,
}

#[async_trait::async_trait]
impl TelegramBindingRepository for Bindings {
    async fn get_active_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        self.active
            .lock()
            .unwrap()
            .get(&telegram_id)
            .copied()
            .ok_or(DomainError::TelegramBindingNotFound)
    }
    async fn store_pending_binding(
        &self,
        telegram_id: i64,
        user_id: Uuid,
    ) -> Result<(), DomainError> {
        self.pending.lock().unwrap().insert(telegram_id, user_id);
        Ok(())
    }
    async fn activate_binding(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        let id = self
            .pending
            .lock()
            .unwrap()
            .remove(&telegram_id)
            .ok_or(DomainError::TelegramBindingNotFound)?;
        self.active.lock().unwrap().insert(telegram_id, id);
        Ok(id)
    }
    async fn get_pending_user_by_telegram_id(&self, telegram_id: i64) -> Result<Uuid, DomainError> {
        self.pending
            .lock()
            .unwrap()
            .get(&telegram_id)
            .copied()
            .ok_or(DomainError::TelegramBindingNotFound)
    }
}

#[derive(Default)]
struct Sessions(Mutex<HashMap<i64, TelegramSessionState>>);

#[async_trait::async_trait]
impl TelegramSessionRepository for Sessions {
    async fn get_state(&self, telegram_id: i64) -> Result<TelegramSessionState, DomainError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&telegram_id)
            .copied()
            .unwrap_or(TelegramSessionState::Unauthenticated))
    }
    async fn set_state(
        &self,
        telegram_id: i64,
        state: TelegramSessionState,
        _ttl: u64,
    ) -> Result<(), DomainError> {
        self.0.lock().unwrap().insert(telegram_id, state);
        Ok(())
    }
    async fn delete(&self, telegram_id: i64) -> Result<(), DomainError> {
        self.0.lock().unwrap().remove(&telegram_id);
        Ok(())
    }
}

/// user → (code, wrong entries); plus users within the resend cooldown.
#[derive(Default)]
struct Codes {
    codes: Mutex<HashMap<Uuid, (String, u32)>>,
    cooling: Mutex<HashSet<Uuid>>,
}

impl Codes {
    fn current(&self, user_id: Uuid) -> Option<String> {
        self.codes
            .lock()
            .unwrap()
            .get(&user_id)
            .map(|(c, _)| c.clone())
    }
    fn expire_cooldown(&self, user_id: Uuid) {
        self.cooling.lock().unwrap().remove(&user_id);
    }
}

#[async_trait::async_trait]
impl AuthCodeStore for Codes {
    async fn store(&self, user_id: Uuid, code: &str, _ttl: u64) -> Result<(), DomainError> {
        self.codes
            .lock()
            .unwrap()
            .insert(user_id, (code.to_string(), 0));
        Ok(())
    }
    async fn verify_and_consume(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError> {
        Ok(self.check_and_consume(user_id, code).await? == CodeCheck::Valid)
    }
    async fn check_and_consume(&self, user_id: Uuid, code: &str) -> Result<CodeCheck, DomainError> {
        let mut codes = self.codes.lock().unwrap();
        let Some((stored, wrong)) = codes.get_mut(&user_id) else {
            return Ok(CodeCheck::Gone);
        };
        if stored == code {
            codes.remove(&user_id);
            return Ok(CodeCheck::Valid);
        }
        *wrong += 1;
        if *wrong >= MAX_WRONG {
            codes.remove(&user_id);
            return Ok(CodeCheck::Gone);
        }
        Ok(CodeCheck::Wrong {
            attempts_left: Some(MAX_WRONG - *wrong),
        })
    }
    async fn try_acquire_send_slot(
        &self,
        user_id: Uuid,
        _cooldown: u64,
    ) -> Result<bool, DomainError> {
        Ok(self.cooling.lock().unwrap().insert(user_id))
    }
    async fn release_send_slot(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.cooling.lock().unwrap().remove(&user_id);
        Ok(())
    }
}

/// Codes 100001, 100002, ... so each email has a new one.
#[derive(Default)]
struct Counter(Mutex<u32>);

impl CodeGenerator for Counter {
    fn generate(&self) -> String {
        let mut n = self.0.lock().unwrap();
        *n += 1;
        format!("{}", 100_000 + *n)
    }
}

#[derive(Default)]
struct Outbox(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl CodeSender for Outbox {
    async fn send_login_code(&self, email: &str, _code: &str) -> Result<(), DomainError> {
        self.0.lock().unwrap().push(email.to_string());
        Ok(())
    }
}

struct World {
    auth: TelegramAuthUseCase,
    codes: Arc<Codes>,
    outbox: Arc<Outbox>,
    user: Uuid,
}

impl World {
    fn new() -> Self {
        domain::settings::init(domain::settings::Settings::mock());
        let user = User::try_new(
            Uuid::new_v4(),
            "s27b_ivanov".to_string(),
            UserRole::Student,
            "Иванов".to_string(),
            "Иван".to_string(),
            None,
            None,
        )
        .unwrap();
        let id = user.id;
        let codes = Arc::new(Codes::default());
        let outbox = Arc::new(Outbox::default());
        let service = Arc::new(OneTimeCodeService::new(
            Arc::new(Counter::default()),
            codes.clone(),
            outbox.clone(),
        ));
        let auth = TelegramAuthUseCase::new(
            Arc::new(Users(user)),
            Arc::new(Bindings::default()),
            Arc::new(Sessions::default()),
            service,
        );
        Self {
            auth,
            codes,
            outbox,
            user: id,
        }
    }

    async fn run(&self, action: TelegramAuthAction) -> Result<TelegramAuthResult, DomainError> {
        self.auth
            .execute(TelegramAuthCommand {
                telegram_id: TG,
                action,
            })
            .await
    }

    async fn code(&self, code: &str) -> TelegramAuthResult {
        self.run(TelegramAuthAction::VerifyCode {
            code: code.to_string(),
        })
        .await
        .unwrap()
    }

    async fn email(&self, email: &str) -> TelegramAuthResult {
        self.run(TelegramAuthAction::SubmitEmail {
            email: email.to_string(),
        })
        .await
        .unwrap()
    }

    async fn resend(&self) -> TelegramAuthResult {
        self.run(TelegramAuthAction::ResendCode).await.unwrap()
    }

    /// /start + email: the bot now waits for the code.
    async fn to_code_step(&self) {
        assert_eq!(
            self.run(TelegramAuthAction::Start).await.unwrap(),
            TelegramAuthResult::AwaitingEmail
        );
        assert_eq!(
            self.email("s27b_ivanov@179.ru").await,
            TelegramAuthResult::CodeGenerated
        );
    }
}

#[tokio::test]
async fn wrong_code_can_be_typed_again() {
    let w = World::new();
    w.to_code_step().await;
    let good = w.codes.current(w.user).unwrap();

    assert_eq!(
        w.code("000000").await,
        TelegramAuthResult::WrongCode {
            attempts_left: Some(4)
        }
    );
    assert_eq!(
        w.code("000001").await,
        TelegramAuthResult::WrongCode {
            attempts_left: Some(3)
        }
    );
    // Still on the code step: the right code logs in, no /start needed.
    assert!(matches!(
        w.code(&good).await,
        TelegramAuthResult::Authenticated { .. }
    ));
}

#[tokio::test]
async fn five_wrong_codes_burn_it() {
    let w = World::new();
    w.to_code_step().await;
    let good = w.codes.current(w.user).unwrap();

    for left in (1..=4).rev() {
        assert_eq!(
            w.code("000000").await,
            TelegramAuthResult::WrongCode {
                attempts_left: Some(left)
            }
        );
    }
    assert_eq!(w.code("000000").await, TelegramAuthResult::CodeExpired);
    // Burned: even the right code no longer works (no brute force by persistence).
    assert_eq!(w.code(&good).await, TelegramAuthResult::CodeExpired);
}

#[tokio::test]
async fn resend_sends_a_new_code_to_the_same_email() {
    let w = World::new();
    w.to_code_step().await;
    let old = w.codes.current(w.user).unwrap();

    // Within the minute: no second email, the first code still works.
    assert_eq!(w.resend().await, TelegramAuthResult::ResendTooSoon);
    assert_eq!(w.outbox.0.lock().unwrap().len(), 1);

    w.codes.expire_cooldown(w.user);
    assert_eq!(w.resend().await, TelegramAuthResult::CodeResent);
    let sent = w.outbox.0.lock().unwrap().clone();
    assert_eq!(sent, vec!["s27b_ivanov@179.ru".to_string(); 2]);

    let new = w.codes.current(w.user).unwrap();
    assert_ne!(old, new);
    assert_eq!(
        w.code(&old).await,
        TelegramAuthResult::WrongCode {
            attempts_left: Some(4)
        }
    );
    assert!(matches!(
        w.code(&new).await,
        TelegramAuthResult::Authenticated { .. }
    ));
}

#[tokio::test]
async fn burned_code_is_replaced_by_resend() {
    let w = World::new();
    w.to_code_step().await;
    for _ in 0..5 {
        w.code("000000").await;
    }
    w.codes.expire_cooldown(w.user);
    assert_eq!(w.resend().await, TelegramAuthResult::CodeResent);
    // A new code gets a fresh set of attempts.
    assert_eq!(
        w.code("000000").await,
        TelegramAuthResult::WrongCode {
            attempts_left: Some(4)
        }
    );
    let new = w.codes.current(w.user).unwrap();
    assert!(matches!(
        w.code(&new).await,
        TelegramAuthResult::Authenticated { .. }
    ));
}

#[tokio::test]
async fn text_on_the_code_step_is_not_a_reset() {
    let w = World::new();
    w.to_code_step().await;
    let good = w.codes.current(w.user).unwrap();

    assert_eq!(w.email("привет").await, TelegramAuthResult::NotACode);
    // A corrected email on the code step is accepted (within the minute: the code
    // already sent is kept, no second email).
    assert_eq!(
        w.email("s27b_ivanov@179.ru").await,
        TelegramAuthResult::ResendTooSoon
    );
    assert_eq!(w.outbox.0.lock().unwrap().len(), 1);
    assert!(matches!(
        w.code(&good).await,
        TelegramAuthResult::Authenticated { .. }
    ));
}

#[tokio::test]
async fn resend_without_login_in_progress_asks_for_start() {
    let w = World::new();
    assert_eq!(w.resend().await, TelegramAuthResult::InvalidState);
    assert!(w.outbox.0.lock().unwrap().is_empty());
}
