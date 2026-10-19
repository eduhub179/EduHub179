use std::sync::Arc;

use application::use_cases::{
    auth::{login::LoginUseCase, OneTimeCodeService},
    homework::{
        create_homework::CreateHomeworkUseCase,
        update_homework_text::UpdateHomeworkTextUseCase,
    },
    schedule::ScheduleService,
};
use axum::{
    http::{HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use domain::{
    errors::DomainError,
    ports::session_security::{SessionTokenGenerator, SessionTokenHasher},
    repositories::{
        class_repository::ClassRepository,
        telegram_bindigs_repository::TelegramBindingRepository,
        user_repository::UserRepository,
        web_session_repository::WebSessionRepository,
    },
};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::routes::{auth, classes, homework, schedule, telegram};

#[derive(Debug)]
pub struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl ApiError {
    pub(crate) fn from_auth_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Invalid login or password")
            }
            DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "User account is inactive"),
            DomainError::InvalidLoginFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid login format")
            }
            DomainError::InvalidEmailFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid email format")
            }
            DomainError::InvalidAuthCode => {
                (StatusCode::UNAUTHORIZED, "Invalid or expired auth code")
            }
            DomainError::UserNotFound => (StatusCode::UNAUTHORIZED, "Invalid login or password"),
            DomainError::InternalError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }

    pub(crate) fn from_schedule_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::UserNotFound => (StatusCode::NOT_FOUND, "Student not found"),
            DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "User account is inactive"),
            DomainError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            DomainError::ScheduleWeekNotFound => {
                (StatusCode::NOT_FOUND, "Schedule week not found")
            }
            DomainError::InvalidLessonInstanceDate => {
                (StatusCode::BAD_REQUEST, "Invalid lesson date")
            }
            DomainError::InternalError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }

    pub(crate) fn from_homework_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::InvalidHomeworkTextFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid homework text format")
            }
            DomainError::HomeworkAlreadyExists => {
                (StatusCode::CONFLICT, "Homework already exists for this lesson")
            }
            DomainError::LessonInstanceNotFound => {
                (StatusCode::NOT_FOUND, "Lesson instance not found")
            }
            DomainError::HomeworkNotFound => (StatusCode::NOT_FOUND, "Homework not found"),
            DomainError::InvalidHomeworkStatus => {
                (StatusCode::BAD_REQUEST, "Invalid homework status")
            }
            DomainError::InvalidNameFormat => (StatusCode::BAD_REQUEST, "Invalid role format"),
            DomainError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            DomainError::InternalError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(json!({
            "error": self.message,
        }));

        (self.status, body).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub login_use_case: Option<Arc<LoginUseCase>>,
    pub schedule_service: Option<Arc<ScheduleService>>,
    pub create_homework_use_case: Option<Arc<CreateHomeworkUseCase>>,
    pub update_homework_use_case: Option<Arc<UpdateHomeworkTextUseCase>>,
    pub class_repository: Option<Arc<dyn ClassRepository>>,
    pub user_repository: Option<Arc<dyn UserRepository>>,
    pub one_time_code_service: Option<Arc<OneTimeCodeService>>,
    pub web_session_repository: Option<Arc<dyn WebSessionRepository>>,
    pub session_token_hasher: Option<Arc<dyn SessionTokenHasher>>,
    pub session_token_generator: Option<Arc<dyn SessionTokenGenerator>>,
    pub telegram_lookup: Option<TelegramLookup>,
}

#[derive(Clone)]
pub struct TelegramLookup {
    pub bindings: Arc<dyn TelegramBindingRepository>,
    pub users: Arc<dyn UserRepository>,
    pub agent_key: String,
}

fn cors_layer() -> CorsLayer {
    let allow_origin = [
        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
        "http://localhost:5174".parse::<HeaderValue>().unwrap(),
        "http://127.0.0.1:5173".parse::<HeaderValue>().unwrap(),
        "http://127.0.0.1:5174".parse::<HeaderValue>().unwrap(),
    ];

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderName::from_static("x-internal-secret"),
        ])
        .allow_credentials(true)
}

pub fn create_app() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .layer(cors_layer())
}

pub fn create_app_with_auth(login_use_case: Arc<LoginUseCase>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/code/send", post(auth::send_auth_code_handler))
        .route("/api/auth/code/verify", post(auth::verify_auth_code_handler))
        .route("/api/schedule/week", get(schedule::schedule_week_handler))
        .route("/api/schedule/day", get(schedule::day_schedule_handler))
        .route(
            "/api/classes/schedule/week",
            get(schedule::class_schedule_week_handler),
        )
        .route(
            "/api/classes/schedule/day",
            get(schedule::class_day_schedule_handler),
        )
        .route("/api/homework", post(homework::create_homework_handler))
        .route(
            "/api/homework/:homework_id/text",
            patch(homework::update_homework_text_handler),
        )
        .route("/api/classes", get(classes::classes_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: Some(login_use_case),
            schedule_service: None,
            create_homework_use_case: None,
            update_homework_use_case: None,
            class_repository: None,
            user_repository: None,
            one_time_code_service: None,
            web_session_repository: None,
            session_token_hasher: None,
            session_token_generator: None,
            telegram_lookup: None,
        })
}

pub fn create_app_with_schedule(schedule_service: Arc<ScheduleService>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/schedule/week", get(schedule::schedule_week_handler))
        .route("/api/schedule/day", get(schedule::day_schedule_handler))
        .route(
            "/api/classes/schedule/week",
            get(schedule::class_schedule_week_handler),
        )
        .route(
            "/api/classes/schedule/day",
            get(schedule::class_day_schedule_handler),
        )
        .route("/api/homework", post(homework::create_homework_handler))
        .route(
            "/api/homework/:homework_id/text",
            patch(homework::update_homework_text_handler),
        )
        .route("/api/classes", get(classes::classes_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: None,
            schedule_service: Some(schedule_service),
            create_homework_use_case: None,
            update_homework_use_case: None,
            class_repository: None,
            user_repository: None,
            one_time_code_service: None,
            web_session_repository: None,
            session_token_hasher: None,
            session_token_generator: None,
            telegram_lookup: None,
        })
}

pub fn create_app_with_auth_and_schedule(
    login_use_case: Arc<LoginUseCase>,
    schedule_service: Arc<ScheduleService>,
    create_homework_use_case: Arc<CreateHomeworkUseCase>,
    update_homework_use_case: Arc<UpdateHomeworkTextUseCase>,
    class_repository: Arc<dyn ClassRepository>,
    user_repository: Arc<dyn UserRepository>,
    one_time_code_service: Arc<OneTimeCodeService>,
    web_session_repository: Arc<dyn WebSessionRepository>,
    session_token_hasher: Arc<dyn SessionTokenHasher>,
    session_token_generator: Arc<dyn SessionTokenGenerator>,
    telegram_lookup: Option<TelegramLookup>,
) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/code/send", post(auth::send_auth_code_handler))
        .route("/api/auth/code/verify", post(auth::verify_auth_code_handler))
        .route("/api/schedule/week", get(schedule::schedule_week_handler))
        .route("/api/schedule/day", get(schedule::day_schedule_handler))
        .route(
            "/api/classes/schedule/week",
            get(schedule::class_schedule_week_handler),
        )
        .route(
            "/api/classes/schedule/day",
            get(schedule::class_day_schedule_handler),
        )
        .route("/api/homework", post(homework::create_homework_handler))
        .route(
            "/api/homework/:homework_id/text",
            patch(homework::update_homework_text_handler),
        )
        .route("/api/classes", get(classes::classes_handler))
        .route("/api/telegram/users/:telegram_id", get(telegram::telegram_user_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: Some(login_use_case),
            schedule_service: Some(schedule_service),
            create_homework_use_case: Some(create_homework_use_case),
            update_homework_use_case: Some(update_homework_use_case),
            class_repository: Some(class_repository),
            user_repository: Some(user_repository),
            one_time_code_service: Some(one_time_code_service),
            web_session_repository: Some(web_session_repository),
            session_token_hasher: Some(session_token_hasher),
            session_token_generator: Some(session_token_generator),
            telegram_lookup,
        })
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "eduhub-api"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::use_cases::auth::login::LoginUseCase;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use domain::entities::user::User;
    use domain::repositories::user_repository::UserRepository;
    use domain::value_objects::role::UserRole;
    use tower::ServiceExt;
    use uuid::Uuid;

    struct MockUserRepository {
        user: Option<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_by_id(&self, _user_id: Uuid) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_active_students_by_class(
            &self,
            _class_id: Uuid,
        ) -> Result<Vec<User>, DomainError> {
            Ok(vec![])
        }

        async fn save(&self, _user: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    struct MockCredentialsStore {
        is_valid: bool,
    }

    #[async_trait::async_trait]
    impl domain::ports::auth::CredentialsStore for MockCredentialsStore {
        async fn verify_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<bool, DomainError> {
            Ok(self.is_valid)
        }

        async fn set_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct MockTokenIssuer {
        token: String,
    }

    impl domain::ports::auth::TokenIssuer for MockTokenIssuer {
        fn issue(&self, _user: &User) -> Result<String, DomainError> {
            Ok(self.token.clone())
        }

        fn verify(&self, _token: &str) -> Result<domain::ports::auth::TokenClaims, DomainError> {
            unimplemented!()
        }
    }

    fn create_test_user() -> User {
        User::try_new(
            Uuid::new_v4(),
            "test_login".to_string(),
            UserRole::Student,
            "Ivanov".to_string(),
            "Ivan".to_string(),
            None,
            None,
        )
        .expect("Test user should be valid")
    }

    #[tokio::test]
    async fn health_route_works() {
        let app = create_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn login_route_works() {
        let user = create_test_user();
        let app = create_app_with_auth(Arc::new(LoginUseCase::new(
            Arc::new(MockUserRepository {
                user: Some(user.clone()),
            }),
            Arc::new(MockCredentialsStore { is_valid: true }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        )));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"login":"test_login","password":"correct_password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
