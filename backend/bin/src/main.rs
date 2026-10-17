use std::sync::Arc;

use application::use_cases::{
    auth::login::LoginUseCase,
    homework::{
        create_homework::CreateHomeworkUseCase,
        update_homework_text::UpdateHomeworkTextUseCase,
    },
    schedule::ScheduleService,
};
use infrastructure::{
    auth::{Argon2PasswordHasher, JwtTokenIssuer},
    config::Config,
    postgres::{
        create_pool, CabinetRepositoryPg, ClassRepositoryPg, CredentialsStorePg,
        HomeworkRepositoryPg, LessonInstanceRepositoryPg, LessonRepositoryPg,
        LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg, StudentGroupRepositoryPg,
        SubjectRepositoryPg, UserRepositoryPg,
    },
};
use presentation::create_app_with_auth_and_schedule;

#[tokio::main]
async fn main() {
    let config = Config::load();

    let pool = create_pool(&config.database_url)
        .await
        .expect("failed to create PostgreSQL pool");

    let class_repo = Arc::new(ClassRepositoryPg::new(pool.clone()));

    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let password_hasher = Arc::new(Argon2PasswordHasher);
    let credentials_store = Arc::new(CredentialsStorePg::new(pool.clone(), password_hasher));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        config.jwt_secret.clone(),
        config.jwt_ttl_seconds as i64,
    ));

    let login_use_case = Arc::new(LoginUseCase::new(user_repo, credentials_store, token_issuer));

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
    ));

    let create_homework_use_case = Arc::new(CreateHomeworkUseCase::new(homeworks.clone()));
    let update_homework_use_case = Arc::new(UpdateHomeworkTextUseCase::new(homeworks));

    let app = create_app_with_auth_and_schedule(
        login_use_case,
        schedule_service,
        create_homework_use_case,
        update_homework_use_case,
        class_repo,
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
