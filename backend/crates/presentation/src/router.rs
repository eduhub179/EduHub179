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
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use domain::{
    entities::user::User,
    errors::DomainError,
    ports::auth::TokenIssuer,
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
use uuid::Uuid;

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
    pub token_issuer: Option<Arc<dyn TokenIssuer>>,
    pub session_token_hasher: Option<Arc<dyn SessionTokenHasher>>,
    pub session_token_generator: Option<Arc<dyn SessionTokenGenerator>>,
    pub telegram_lookup: Option<TelegramLookup>,
    pub agent_access: Option<AgentAccess>,
}

#[derive(Clone)]
pub struct TelegramLookup {
    pub bindings: Arc<dyn TelegramBindingRepository>,
    pub users: Arc<dyn UserRepository>,
}

/// Header with the shared secret of the chat agent (`AGENT_KEY` in backend/.env and agent/.env).
const AGENT_KEY_HEADER: &str = "x-agent-key";
/// Optional header: the agent acts as this user (UUID) instead of its own service user,
/// e.g. for homework a logged-in user sent the bot in DM.
const AGENT_ON_BEHALF_OF_HEADER: &str = "x-agent-on-behalf-of";

/// Access to the API for the chat agent (Python service in `agent/`), which has no browser session.
///
/// Invariant: the key is never empty, so an unset `AGENT_KEY` cannot match a missing header.
#[derive(Clone)]
pub struct AgentAccess {
    key: String,
    /// Service user the agent acts as by default (created by 0008_create_agent_user.sql).
    user_id: Uuid,
}

impl AgentAccess {
    /// Returns `None` for an empty key: agent access is then off.
    pub fn new(key: String, user_id: Uuid) -> Option<Self> {
        (!key.is_empty()).then_some(Self { key, user_id })
    }

    /// Constant-time comparison, so response timing does not leak the key byte by byte.
    /// Only the length can leak, which is acceptable for a random secret.
    fn matches(&self, presented: &str) -> bool {
        let (a, b) = (self.key.as_bytes(), presented.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
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

fn extract_cookie_value(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    let header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;

    header
        .split(';')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, '=');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            (name == cookie_name).then(|| value.to_string())
        })
        .next()
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn unauthorized(message: &str) -> ApiError {
    ApiError {
        status: StatusCode::UNAUTHORIZED,
        message: message.to_string(),
    }
}

/// Checks `X-Agent-Key` for agent-only routes.
///
/// # Fail-safe поведение
/// Returns `Err(401)` if agent access is off, the header is missing or the key is wrong.
pub(crate) fn require_agent<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
) -> Result<&'a AgentAccess, ApiError> {
    let agent = state
        .agent_access
        .as_ref()
        .ok_or_else(|| unauthorized("Agent key required"))?;
    match header_value(headers, AGENT_KEY_HEADER) {
        Some(key) if agent.matches(key) => Ok(agent),
        _ => Err(unauthorized("Agent key required")),
    }
}

/// Resolves the caller: the chat agent by `X-Agent-Key`, otherwise a web user by the `session` cookie.
///
/// # Fail-safe поведение
/// A present but wrong `X-Agent-Key` is rejected with 401 without falling back to the cookie.
/// Inactive users (including the one in `X-Agent-On-Behalf-Of`) get 403.
pub(crate) async fn require_authenticated_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<User, ApiError> {
    require_caller(state, headers).await.map(|caller| caller.user)
}

/// Authenticated caller of a route.
pub(crate) struct Caller {
    /// User the request acts as (for the agent: its service user or `X-Agent-On-Behalf-Of`).
    pub(crate) user: User,
    /// True only after a valid `X-Agent-Key`: routes may trust the agent beyond the user's role.
    pub(crate) is_agent: bool,
}

/// Same as `require_authenticated_user`, but also tells whether the request came from the agent.
pub(crate) async fn require_caller(state: &AppState, headers: &HeaderMap) -> Result<Caller, ApiError> {
    if headers.contains_key(AGENT_KEY_HEADER) {
        let user = authenticate_agent(state, headers).await?;
        return Ok(Caller { user, is_agent: true });
    }
    let user = authenticate_session(state, headers).await?;
    Ok(Caller { user, is_agent: false })
}

/// The agent acts as its service user or as the user from `X-Agent-On-Behalf-Of`;
/// permission checks in handlers then apply to that user as usual.
async fn authenticate_agent(state: &AppState, headers: &HeaderMap) -> Result<User, ApiError> {
    let agent = require_agent(state, headers)?;
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;

    let user_id = match header_value(headers, AGENT_ON_BEHALF_OF_HEADER) {
        Some(raw) => Uuid::parse_str(raw.trim()).map_err(|_| ApiError {
            status: StatusCode::BAD_REQUEST,
            message: "Invalid X-Agent-On-Behalf-Of".to_string(),
        })?,
        None => agent.user_id,
    };
    let user = match user_repository.get_by_id(user_id).await {
        Ok(user) => user,
        Err(DomainError::UserNotFound) => return Err(unauthorized("Unknown agent user")),
        Err(error) => return Err(ApiError::from_auth_domain(error)),
    };

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    Ok(user)
}

async fn authenticate_session(state: &AppState, headers: &HeaderMap) -> Result<User, ApiError> {
    let web_session_repository =
        state
            .web_session_repository
            .as_ref()
            .ok_or_else(|| ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Web session repository not initialized".to_string(),
            })?;
    let session_token_hasher = state.session_token_hasher.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Session token hasher not initialized".to_string(),
    })?;
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;

    let raw_token = extract_cookie_value(headers, "session").ok_or_else(|| ApiError {
        status: StatusCode::UNAUTHORIZED,
        message: "Authentication required".to_string(),
    })?;
    let token_hash = session_token_hasher
        .hash_session_token(&raw_token)
        .map_err(ApiError::from_auth_domain)?;
    let session = web_session_repository
        .get_active_by_token_hash(&token_hash)
        .await
        .map_err(ApiError::from_auth_domain)?;
    let user = user_repository
        .get_by_id(session.user_id)
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    Ok(user)
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
        .route("/api/auth/session/init", post(auth::init_session_handler))
        .route("/api/auth/logout", post(auth::logout_handler))
        .route("/api/auth/profile/class", patch(auth::update_profile_class_handler))
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
            token_issuer: None,
            session_token_hasher: None,
            session_token_generator: None,
            telegram_lookup: None,
            agent_access: None,
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
            token_issuer: None,
            session_token_hasher: None,
            session_token_generator: None,
            telegram_lookup: None,
            agent_access: None,
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
    token_issuer: Arc<dyn TokenIssuer>,
    session_token_hasher: Arc<dyn SessionTokenHasher>,
    session_token_generator: Arc<dyn SessionTokenGenerator>,
    telegram_lookup: Option<TelegramLookup>,
    agent_access: Option<AgentAccess>,
) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/session/init", post(auth::init_session_handler))
        .route("/api/auth/logout", post(auth::logout_handler))
        .route("/api/auth/profile/class", patch(auth::update_profile_class_handler))
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
            token_issuer: Some(token_issuer),
            session_token_hasher: Some(session_token_hasher),
            session_token_generator: Some(session_token_generator),
            telegram_lookup,
            agent_access,
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

    const AGENT_KEY: &str = "agent-secret";

    /// Returns an active user with exactly the requested id, so tests see whom the agent acts as.
    struct EchoUserRepository;

    #[async_trait::async_trait]
    impl UserRepository for EchoUserRepository {
        async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError> {
            let mut user = create_test_user();
            user.id = user_id;
            Ok(user)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            Err(DomainError::UserNotFound)
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

    fn agent_state(agent_user_id: Uuid) -> AppState {
        AppState {
            login_use_case: None,
            schedule_service: None,
            create_homework_use_case: None,
            update_homework_use_case: None,
            class_repository: None,
            user_repository: Some(Arc::new(EchoUserRepository)),
            one_time_code_service: None,
            web_session_repository: None,
            token_issuer: None,
            session_token_hasher: None,
            session_token_generator: None,
            telegram_lookup: None,
            agent_access: AgentAccess::new(AGENT_KEY.to_string(), agent_user_id),
        }
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn empty_agent_key_disables_agent_access() {
        assert!(AgentAccess::new(String::new(), Uuid::new_v4()).is_none());
    }

    #[tokio::test]
    async fn agent_key_authenticates_as_agent_user() {
        let agent_user_id = Uuid::new_v4();
        let state = agent_state(agent_user_id);

        let user = require_authenticated_user(&state, &headers(&[("x-agent-key", AGENT_KEY)]))
            .await
            .unwrap();

        assert_eq!(user.id, agent_user_id);
    }

    #[tokio::test]
    async fn agent_can_act_on_behalf_of_user() {
        let state = agent_state(Uuid::new_v4());
        let author_id = Uuid::new_v4();

        let user = require_authenticated_user(
            &state,
            &headers(&[
                ("x-agent-key", AGENT_KEY),
                ("x-agent-on-behalf-of", &author_id.to_string()),
            ]),
        )
        .await
        .unwrap();

        assert_eq!(user.id, author_id);
    }

    #[tokio::test]
    async fn agent_caller_is_marked_as_agent() {
        let state = agent_state(Uuid::new_v4());

        let caller = require_caller(&state, &headers(&[("x-agent-key", AGENT_KEY)]))
            .await
            .unwrap();

        assert!(caller.is_agent);
    }

    #[tokio::test]
    async fn wrong_agent_key_is_rejected() {
        let state = agent_state(Uuid::new_v4());

        let error = require_authenticated_user(&state, &headers(&[("x-agent-key", "wrong")]))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn agent_key_is_rejected_when_agent_access_is_off() {
        let mut state = agent_state(Uuid::new_v4());
        state.agent_access = None;

        let error = require_authenticated_user(&state, &headers(&[("x-agent-key", "")]))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn invalid_on_behalf_of_is_bad_request() {
        let state = agent_state(Uuid::new_v4());

        let error = require_authenticated_user(
            &state,
            &headers(&[("x-agent-key", AGENT_KEY), ("x-agent-on-behalf-of", "not-a-uuid")]),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }
}
