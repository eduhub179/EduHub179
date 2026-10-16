//! Composition root for the Telegram bot binary.
//!
//! All configuration comes from `Config::load()` (which reads `.env` via dotenvy).
//! No direct `std::env::var` calls — the composition root stays thin.

use std::sync::Arc;

use application::use_cases::auth::OneTimeCodeService;
use application::use_cases::telegram::TelegramAuthUseCase;
use domain::settings::Settings;
use infrastructure::auth::{Argon2PasswordHasher, LoggingCodeSender, NumericCodeGenerator};
use infrastructure::config::Config;
use infrastructure::postgres::{create_pool, TelegramBindingRepositoryPg, UserRepositoryPg};
use infrastructure::redis::{AuthCodeStoreRedis, TelegramSessionRepositoryRedis};
use telegram_adapter::bot::run_bot;
use telegram_adapter::state::BotState;
use teloxide::requests::Requester;

#[tokio::main]
async fn main() {
    // 1. Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    tracing::info!("Starting EduHub179 Telegram bot...");

    // 2. Load configuration (reads .env via dotenvy).
    let config = Config::load();

    // 3. Inject domain settings (org email domain for login validation).
    let settings = Settings::try_new(config.org_email_domain.clone())
        .expect("ORG_EMAIL_DOMAIN must look like '@179.ru' (leading '@', non-empty)");
    domain::settings::init(settings);

    // 4. Infrastructure connections (pool + Redis).
    let pg_pool = create_pool(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    // 5. Repositories
    let user_repo: Arc<dyn domain::repositories::user_repository::UserRepository> =
        Arc::new(UserRepositoryPg::new(pg_pool.clone()));

    let tg_binding_repo: Arc<
        dyn domain::repositories::telegram_bindigs_repository::TelegramBindingRepository,
    > = Arc::new(TelegramBindingRepositoryPg::new(pg_pool));

    let tg_session_repo: Arc<
        dyn domain::repositories::telegram_session_repository::TelegramSessionRepository,
    > = Arc::new(
        TelegramSessionRepositoryRedis::new(&config.redis_url)
            .await
            .expect("Failed to connect to Redis (sessions)"),
    );

    // 6. Auth code infrastructure (shared hasher for passwords + OTP codes).
    let hasher: Arc<dyn infrastructure::auth::PasswordHasher> = Arc::new(Argon2PasswordHasher);

    let auth_code_store: Arc<dyn domain::ports::auth::AuthCodeStore> = Arc::new(
        AuthCodeStoreRedis::new(&config.redis_url, hasher.clone())
            .await
            .expect("Failed to connect to Redis (auth codes)"),
    );

    let code_generator: Arc<dyn domain::ports::auth::CodeGenerator> =
        Arc::new(NumericCodeGenerator);

    let code_sender: Arc<dyn domain::ports::auth::CodeSender> = Arc::new(LoggingCodeSender);

    // 7. Application services & use cases
    let one_time_code_service = Arc::new(OneTimeCodeService::new(
        code_generator,
        auth_code_store,
        code_sender,
    ));

    let telegram_auth_use_case = Arc::new(TelegramAuthUseCase::new(
        user_repo,
        tg_binding_repo,
        tg_session_repo,
        one_time_code_service,
    ));

    // 8. Telegram bot
    let bot = teloxide::Bot::new(config.telegram_bot_token.clone());

    let bot_state = BotState {
        bot: bot.clone(),
        auth_use_case: telegram_auth_use_case,
        bot_username: bot
            .get_me()
            .await
            .expect("get_me failed")
            .username
            .clone()
            .unwrap_or("".to_string()),
    };

    tracing::info!("Telegram bot polling started");

    // 9. Run (blocks until Ctrl+C via teloxide's built-in handler)
    run_bot(bot_state).await;
}
