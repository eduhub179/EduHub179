//! Integration tests for `EmailLoginService`: real PostgreSQL (users, password hashes),
//! in-memory code store and a mailbox that records sent codes instead of emailing them.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test email_login

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use application::use_cases::auth::email_login::{EmailLoginService, LoginMethod};
use application::use_cases::auth::OneTimeCodeService;
use domain::errors::DomainError;
use domain::ports::auth::{AuthCodeStore, CodeGenerator, CodeSender, CredentialsStore};
use domain::settings::Settings;
use infrastructure::auth::{Argon2PasswordHasher, SecureSessionTokenGenerator};
use infrastructure::postgres::{CredentialsStorePg, UserRepositoryPg};
use sqlx::PgPool;
use uuid::Uuid;

const MAX_WRONG: u32 = 5;

/// Codes and tokens in memory, with the same rules as the Redis store: one per user,
/// consumed on success, burned after 5 wrong entries, one send slot per user.
#[derive(Default)]
struct MemoryStore {
    /// user → (value, wrong attempts)
    values: Mutex<HashMap<Uuid, (String, u32)>>,
    slots: Mutex<HashSet<Uuid>>,
}

impl MemoryStore {
    /// Lets the next send through, as if the cooldown expired.
    fn expire_cooldown(&self, user_id: Uuid) {
        self.slots.lock().unwrap().remove(&user_id);
    }
}

#[async_trait::async_trait]
impl AuthCodeStore for MemoryStore {
    async fn store(&self, user_id: Uuid, code: &str, _ttl: u64) -> Result<(), DomainError> {
        self.values.lock().unwrap().insert(user_id, (code.to_string(), 0));
        Ok(())
    }

    async fn verify_and_consume(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError> {
        let mut values = self.values.lock().unwrap();
        let Some((stored, wrong)) = values.get_mut(&user_id) else { return Ok(false) };
        if stored == code {
            values.remove(&user_id);
            return Ok(true);
        }
        *wrong += 1;
        if *wrong >= MAX_WRONG {
            values.remove(&user_id);
        }
        Ok(false)
    }

    async fn try_acquire_send_slot(&self, user_id: Uuid, _cooldown: u64) -> Result<bool, DomainError> {
        Ok(self.slots.lock().unwrap().insert(user_id))
    }

    async fn release_send_slot(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.slots.lock().unwrap().remove(&user_id);
        Ok(())
    }
}

/// Records (email, code) instead of sending.
#[derive(Default)]
struct Mailbox {
    sent: Mutex<Vec<(String, String)>>,
    /// While true, every send fails (mail server down).
    broken: Mutex<bool>,
}

impl Mailbox {
    fn last_code_to(&self, email: &str) -> Option<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(to, _)| to == email)
            .map(|(_, code)| code.clone())
    }

    fn count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl CodeSender for Mailbox {
    async fn send_login_code(&self, email: &str, code: &str) -> Result<(), DomainError> {
        if *self.broken.lock().unwrap() {
            return Err(DomainError::CodeDeliveryFailed);
        }
        self.sent.lock().unwrap().push((email.to_string(), code.to_string()));
        Ok(())
    }
}

/// Different codes each time, so a stale one never matches by accident.
#[derive(Default)]
struct CountingCodes(Mutex<u32>);

impl CodeGenerator for CountingCodes {
    fn generate(&self) -> String {
        let mut next = self.0.lock().unwrap();
        *next += 1;
        format!("{:06}", 100000 + *next)
    }
}

struct World {
    service: EmailLoginService,
    credentials: Arc<CredentialsStorePg>,
    codes: Arc<MemoryStore>,
    mailbox: Arc<Mailbox>,
    /// Has no password yet.
    newcomer: Uuid,
    /// Another account without a password.
    second: Uuid,
    /// Already has a password.
    regular: Uuid,
    inactive: Uuid,
}

const NEWCOMER: &str = "s27b_novikov@179.ru";
const SECOND: &str = "s27b_orlova@179.ru";
const REGULAR: &str = "teacher_petrova@179.ru";
const INACTIVE: &str = "s20a_old@179.ru";
const PASSWORD: &str = "correct horse";

async fn insert_user(pool: &PgPool, login: &str, role: &str, is_active: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, login, role, last_name, first_name, is_active)
         VALUES ($1, $2, $3::user_role, 'Тестов', 'Тест', $4)",
    )
    .bind(id)
    .bind(login)
    .bind(role)
    .bind(is_active)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed(pool: PgPool) -> World {
    domain::settings::init(Settings::mock());
    let newcomer = insert_user(&pool, "s27b_novikov", "student", true).await;
    let second = insert_user(&pool, "s27b_orlova", "student", true).await;
    let regular = insert_user(&pool, "teacher_petrova", "teacher", true).await;
    let inactive = insert_user(&pool, "s20a_old", "student", false).await;

    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), Arc::new(Argon2PasswordHasher)));
    credentials.set_password(regular, PASSWORD).await.unwrap();

    let codes = Arc::new(MemoryStore::default());
    let mailbox = Arc::new(Mailbox::default());
    let one_time = Arc::new(OneTimeCodeService::new(
        Arc::new(CountingCodes::default()),
        codes.clone(),
        mailbox.clone(),
    ));
    let service = EmailLoginService::new(
        Arc::new(UserRepositoryPg::new(pool)),
        credentials.clone(),
        one_time,
        Arc::new(MemoryStore::default()),
        Arc::new(SecureSessionTokenGenerator),
    );
    World { service, credentials, codes, mailbox, newcomer, second, regular, inactive }
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_login_code_then_mandatory_password_then_password_login(pool: PgPool) {
    let w = seed(pool).await;

    // Mixed case and spaces are fine: the address is normalized.
    let start = w.service.start("  S27B_Novikov@179.RU ").await.unwrap();
    assert_eq!(start.method, LoginMethod::Code);
    assert!(start.code_sent);
    let code = w.mailbox.last_code_to(NEWCOMER).expect("a code was emailed");

    // The code alone does not log in and is not a password.
    assert_eq!(w.service.login(NEWCOMER, &code).await.unwrap_err(), DomainError::PasswordNotSet);

    let token = w.service.verify_code(NEWCOMER, &code).await.unwrap();
    // A weak password does not burn the token.
    let weak = w.service.complete_first_login(NEWCOMER, &token, "short").await;
    assert_eq!(weak.unwrap_err(), DomainError::InvalidPasswordFormat);
    let user = w.service.complete_first_login(NEWCOMER, &token, "мой новый пароль").await.unwrap();
    assert_eq!(user.id, w.newcomer);
    assert!(w.credentials.has_password(w.newcomer).await.unwrap());

    // From now on: password only.
    assert_eq!(w.service.start(NEWCOMER).await.unwrap().method, LoginMethod::Password);
    assert_eq!(w.service.login(NEWCOMER, "мой новый пароль").await.unwrap().id, w.newcomer);
    assert_eq!(w.service.login(NEWCOMER, "wrong password").await.unwrap_err(), DomainError::InvalidCredentials);
    assert_eq!(w.service.resend_code(NEWCOMER).await.unwrap_err(), DomainError::PasswordAlreadySet);
    assert_eq!(w.service.verify_code(NEWCOMER, &code).await.unwrap_err(), DomainError::PasswordAlreadySet);
    // The token was single-use (and the password is set anyway).
    let again = w.service.complete_first_login(NEWCOMER, &token, "another password").await;
    assert_eq!(again.unwrap_err(), DomainError::PasswordAlreadySet);
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_with_password_logs_in_without_code(pool: PgPool) {
    let w = seed(pool).await;

    let start = w.service.start(REGULAR).await.unwrap();
    assert_eq!(start.method, LoginMethod::Password);
    assert_eq!(w.mailbox.count(), 0, "no email for an account with a password");
    assert_eq!(w.service.login(REGULAR, PASSWORD).await.unwrap().id, w.regular);
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_codes_burn_after_five_attempts(pool: PgPool) {
    let w = seed(pool).await;
    w.service.start(NEWCOMER).await.unwrap();
    let code = w.mailbox.last_code_to(NEWCOMER).unwrap();

    for _ in 0..MAX_WRONG {
        let wrong = w.service.verify_code(NEWCOMER, "000000").await;
        assert_eq!(wrong.unwrap_err(), DomainError::InvalidAuthCode);
    }
    // Even the right code is useless now: a new one must be requested.
    assert_eq!(w.service.verify_code(NEWCOMER, &code).await.unwrap_err(), DomainError::InvalidAuthCode);

    w.codes.expire_cooldown(w.newcomer);
    w.service.resend_code(NEWCOMER).await.unwrap();
    let fresh = w.mailbox.last_code_to(NEWCOMER).unwrap();
    assert_ne!(fresh, code);
    assert!(w.service.verify_code(NEWCOMER, &fresh).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn codes_are_sent_at_most_once_per_cooldown(pool: PgPool) {
    let w = seed(pool).await;

    let first = w.service.start(NEWCOMER).await.unwrap();
    // Opening the form again within a minute: still the code step, no second email.
    let second = w.service.start(NEWCOMER).await.unwrap();
    let resend = w.service.resend_code(NEWCOMER).await;

    assert!(first.code_sent);
    assert_eq!(second.method, LoginMethod::Code);
    assert!(!second.code_sent);
    assert_eq!(resend.unwrap_err(), DomainError::AuthCodeCooldown);
    assert_eq!(w.mailbox.count(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn setup_token_belongs_to_its_account(pool: PgPool) {
    let w = seed(pool).await;
    w.service.start(NEWCOMER).await.unwrap();
    let code = w.mailbox.last_code_to(NEWCOMER).unwrap();
    let token = w.service.verify_code(NEWCOMER, &code).await.unwrap();

    let forged = w.service.complete_first_login(NEWCOMER, "not-the-token", "a good password").await;
    assert_eq!(forged.unwrap_err(), DomainError::InvalidAuthCode);
    // A real token, but of another person: it cannot set this account's password.
    let stolen = w.service.complete_first_login(SECOND, &token, "a good password").await;
    assert_eq!(stolen.unwrap_err(), DomainError::InvalidAuthCode);
    assert!(!w.credentials.has_password(w.second).await.unwrap());
    // The owner still can (the forged attempt did not consume it).
    assert!(w.service.complete_first_login(NEWCOMER, &token, "a good password").await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_foreign_and_inactive_emails_are_refused(pool: PgPool) {
    let w = seed(pool).await;

    assert_eq!(w.service.start("nobody@179.ru").await.unwrap_err(), DomainError::UserNotFound);
    assert_eq!(w.service.start("s27b_novikov@gmail.com").await.unwrap_err(), DomainError::InvalidEmailFormat);
    assert_eq!(w.service.start(INACTIVE).await.unwrap_err(), DomainError::UserIsInactive);
    // Login does not tell an unknown email from a wrong password.
    assert_eq!(w.service.login("nobody@179.ru", PASSWORD).await.unwrap_err(), DomainError::InvalidCredentials);
    assert_eq!(w.mailbox.count(), 0);
    assert!(!w.credentials.has_password(w.inactive).await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_email_does_not_start_the_cooldown(pool: PgPool) {
    let w = seed(pool).await;
    *w.mailbox.broken.lock().unwrap() = true;

    assert_eq!(w.service.start(NEWCOMER).await.unwrap_err(), DomainError::CodeDeliveryFailed);

    // The mail server is back: the next try sends at once instead of "wait a minute".
    *w.mailbox.broken.lock().unwrap() = false;
    let retry = w.service.start(NEWCOMER).await.unwrap();
    assert!(retry.code_sent);
    assert!(w.mailbox.last_code_to(NEWCOMER).is_some());
}
