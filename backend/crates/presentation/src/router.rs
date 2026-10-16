//! Axum router for the HTTP API.
//!
//! This is the initial skeleton. It exposes a minimal `/health` route and can be
//! expanded with feature-specific endpoints (`auth`, `homework`, `schedule`, etc.).

use application::use_cases::auth::{
    login::{LoginCommand, LoginUseCase},
    AuthSession,
};
use axum::{
    extract::State,
    http::{StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: PublicUser,
}

#[derive(Debug, Serialize)]
pub struct PublicUser {
    pub id: String,
    pub login: String,
    pub role: String,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: Option<String>,
    pub is_active: bool,
    pub class_id: Option<String>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn from_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid login or password"),
            DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "User account is inactive"),
            DomainError::InvalidLoginFormat => (StatusCode::UNPROCESSABLE_ENTITY, "Invalid login format"),
            DomainError::UserNotFound => (StatusCode::UNAUTHORIZED, "Invalid login or password"),
            DomainError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
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

pub fn create_app() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
}

pub fn create_app_with_auth(login_use_case: Arc<LoginUseCase>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(login_handler))
        .with_state(login_use_case)
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "eduhub-api"
    }))
}

async fn login_handler(
    State(login_use_case): State<Arc<LoginUseCase>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let command = LoginCommand::try_new(payload.login, payload.password)
        .map_err(ApiError::from_domain)?;

    let session = login_use_case
        .execute(command)
        .await
        .map_err(ApiError::from_domain)?;

    Ok(Json(LoginResponse {
        token: session.token,
        user: PublicUser {
            id: session.user.id.to_string(),
            login: session.user.login,
            role: session.user.role.to_string(),
            first_name: session.user.first_name,
            last_name: session.user.last_name,
            middle_name: session.user.middle_name,
            is_active: session.user.is_active,
            class_id: session.user.class_id.map(|id| id.to_string()),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::use_cases::auth::login::{LoginCommand, LoginUseCase};
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
            Arc::new(MockUserRepository { user: Some(user.clone()) }),
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
