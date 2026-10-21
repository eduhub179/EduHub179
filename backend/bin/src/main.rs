use std::sync::Arc;

use application::use_cases::{
    auth::OneTimeCodeService,
    homework::{
        attach_file_to_lesson::AttachFileToLessonUseCase,
        create_homework::CreateHomeworkUseCase,
        delete_file::DeleteHomeworkFileUseCase,
        delete_homework::DeleteHomeworkUseCase,
        download_file::DownloadHomeworkFileUseCase,
        update_homework_text::UpdateHomeworkTextUseCase,
    },
    lesson_access::LessonAccess,
    materials::MaterialsService,
    plusnik::PlusnikService,
    schedule::ScheduleService,
};
use domain::ports::auth::TokenIssuer;
use domain::ports::file_storage::FileStorage;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::ports::session_security::{SessionTokenGenerator, SessionTokenHasher};
use domain::repositories::web_session_repository::WebSessionRepository;
use domain::settings::Settings;
use infrastructure::{
    auth::{
        Argon2PasswordHasher, HmacSessionTokenHasher, JwtTokenIssuer, LoggingCodeSender,
        NumericCodeGenerator, SecureSessionTokenGenerator,
    },
    config::Config,
    postgres::{
        create_pool, CabinetRepositoryPg, ClassRepositoryPg, CredentialsStorePg,
        HomeworkRepositoryPg, LessonInstanceRepositoryPg, LessonMaterialRepositoryPg,
        LessonRepositoryPg, PlusnikRepositoryPg,
        LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg, StudentGroupRepositoryPg,
        SubjectRepositoryPg, TelegramBindingRepositoryPg, UserRepositoryPg,
        WebSessionRepositoryPg,
    },
    redis::AuthCodeStoreRedis,
    storage::S3FileStorage,
};
use presentation::{create_app_with_auth_and_schedule, AgentAccess, FileUseCases, TelegramLookup};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let config = Config::load();
    let settings = Settings::try_new(config.org_email_domain.clone())
        .expect("ORG_EMAIL_DOMAIN must look like '@179.ru'");
    domain::settings::init(settings);

    let pool = create_pool(&config.database_url)
        .await
        .expect("failed to create PostgreSQL pool");

    let class_repo = Arc::new(ClassRepositoryPg::new(pool.clone()));
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let password_hasher = Arc::new(Argon2PasswordHasher);
    let credentials_store = Arc::new(CredentialsStorePg::new(pool.clone(), password_hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        config.jwt_secret.clone(),
        config.jwt_ttl_seconds as i64,
    ));

    let login_use_case = Arc::new(application::use_cases::auth::login::LoginUseCase::new(
        user_repo.clone(),
        credentials_store,
        token_issuer.clone(),
    ));

    let redis_hasher = Arc::new(Argon2PasswordHasher);
    let auth_code_store = Arc::new(
        AuthCodeStoreRedis::new(&config.redis_url, redis_hasher)
            .await
            .expect("failed to connect to Redis auth code store"),
    );
    let code_generator = Arc::new(NumericCodeGenerator);
    let code_sender = Arc::new(LoggingCodeSender);
    let one_time_code_service = Arc::new(OneTimeCodeService::new(
        code_generator,
        auth_code_store,
        code_sender,
    ));

    let web_session_repository: Arc<dyn WebSessionRepository> =
        Arc::new(WebSessionRepositoryPg::new(pool.clone()));
    let session_token_hasher: Arc<dyn SessionTokenHasher> = Arc::new(
        HmacSessionTokenHasher::try_new(&config.session_hash_secret)
            .expect("SESSION_HASH_SECRET must be set and non-empty"),
    );
    let session_token_generator: Arc<dyn SessionTokenGenerator> =
        Arc::new(SecureSessionTokenGenerator);

    let schedule_weeks = Arc::new(ScheduleWeekRepositoryPg::new(pool.clone()));
    let student_groups = Arc::new(StudentGroupRepositoryPg::new(pool.clone()));
    let instances = Arc::new(LessonInstanceRepositoryPg::new(pool.clone()));
    let templates = Arc::new(LessonTemplateRepositoryPg::new(pool.clone()));
    let lessons = Arc::new(LessonRepositoryPg::new(pool.clone()));
    let subjects = Arc::new(SubjectRepositoryPg::new(pool.clone()));
    let users = Arc::new(UserRepositoryPg::new(pool.clone()));
    let cabinets = Arc::new(CabinetRepositoryPg::new(pool.clone()));
    let homeworks = Arc::new(HomeworkRepositoryPg::new(pool.clone()));

    let schedule_service = Arc::new(ScheduleService::new(
        schedule_weeks,
        student_groups,
        instances,
        templates,
        lessons,
        subjects,
        users,
        cabinets,
        homeworks.clone(),
        Arc::new(ClassRepositoryPg::new(pool.clone())),
    ));

    let create_homework_use_case = Arc::new(CreateHomeworkUseCase::new(homeworks.clone()));
    let update_homework_use_case = Arc::new(UpdateHomeworkTextUseCase::new(homeworks.clone()));

    // Default is the fixed id of the agent user from 0008_create_agent_user.sql.
    let agent_user_id = std::env::var("AGENT_USER_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .map(|id| Uuid::parse_str(id.trim()).expect("AGENT_USER_ID must be a UUID"))
        .unwrap_or(Uuid::from_u128(0xc1));
    let agent_access =
        AgentAccess::new(std::env::var("AGENT_KEY").unwrap_or_default(), agent_user_id);
    let telegram_lookup = agent_access.is_some().then(|| TelegramLookup {
        bindings: Arc::new(TelegramBindingRepositoryPg::new(pool.clone())),
        users: Arc::new(UserRepositoryPg::new(pool.clone())),
    });

    // Object storage for homework files. Without S3_* the file routes answer 503.
    // The bucket is checked/created here, so a wrong config shows up at startup;
    // MinIO being down does not block the rest of the API.
    let homeworks: Arc<dyn HomeworkRepository> = homeworks;
    let mut file_storage_for_cleanup: Option<Arc<dyn FileStorage>> = None;
    let file_use_cases = match &config.s3 {
        Some(s3) => {
            let file_storage = S3FileStorage::new(s3);
            match file_storage.ensure_bucket().await {
                Ok(()) => println!("file storage: bucket '{}' at {} is ready", s3.bucket, s3.endpoint),
                Err(_) => eprintln!(
                    "file storage: cannot reach bucket '{}' at {} (is MinIO running? check S3_* in .env)",
                    s3.bucket, s3.endpoint
                ),
            }
            let storage: Arc<dyn FileStorage> = Arc::new(file_storage);
            file_storage_for_cleanup = Some(storage.clone());
            Some(FileUseCases {
                attach_to_lesson: Arc::new(AttachFileToLessonUseCase::new(
                    homeworks.clone(),
                    storage.clone(),
                )),
                download: Arc::new(DownloadHomeworkFileUseCase::new(
                    homeworks.clone(),
                    storage.clone(),
                )),
                delete: Arc::new(DeleteHomeworkFileUseCase::new(homeworks.clone(), storage)),
            })
        }
        None => {
            println!("file storage: off (S3_ENDPOINT is not set), file routes answer 503");
            None
        }
    };

    let plusnik_service = Arc::new(PlusnikService::new(
        Arc::new(PlusnikRepositoryPg::new(pool.clone())),
        Arc::new(LessonRepositoryPg::new(pool.clone())),
        Arc::new(UserRepositoryPg::new(pool.clone())),
        Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
        Arc::new(SubjectRepositoryPg::new(pool.clone())),
        Arc::new(ClassRepositoryPg::new(pool.clone())),
    ));

    // Links work without S3; file uploads/downloads answer 503 then.
    let materials_service = Arc::new(MaterialsService::new(
        Arc::new(LessonMaterialRepositoryPg::new(pool.clone())),
        LessonAccess::new(
            Arc::new(LessonRepositoryPg::new(pool.clone())),
            Arc::new(UserRepositoryPg::new(pool.clone())),
            Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
            Arc::new(SubjectRepositoryPg::new(pool.clone())),
            Arc::new(ClassRepositoryPg::new(pool.clone())),
        ),
        file_storage_for_cleanup.clone(),
    ));

    let app = create_app_with_auth_and_schedule(
        login_use_case,
        schedule_service,
        create_homework_use_case,
        update_homework_use_case,
        class_repo,
        user_repo.clone(),
        one_time_code_service,
        web_session_repository,
        token_issuer as Arc<dyn TokenIssuer>,
        session_token_hasher,
        session_token_generator,
        telegram_lookup,
        agent_access,
        file_use_cases,
        Arc::new(DeleteHomeworkUseCase::new(homeworks, file_storage_for_cleanup)),
        plusnik_service,
        materials_service,
    );

    let addr = format!("{}:{}", config.server_host, config.server_port);
    println!("starting EduHub179 API on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind server port");

    axum::serve(listener, app.into_make_service())
        .await
        .expect("server failed");
}
