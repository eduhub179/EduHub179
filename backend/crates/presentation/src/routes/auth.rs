use application::use_cases::auth::login::{LoginCommand, LoginUseCase};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::router::{AppState, ApiError};

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

pub fn auth_router() -> Router {
    Router::new().route("/api/auth/login", post(login_handler))
}

async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let login_use_case = state.login_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Login use case not initialized".to_string(),
    })?;

    let command = LoginCommand::try_new(payload.login, payload.password)
        .map_err(ApiError::from_auth_domain)?;

    let session = login_use_case
        .execute(command)
        .await
        .map_err(ApiError::from_auth_domain)?;

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
