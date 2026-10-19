use application::use_cases::auth::login::LoginCommand;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use domain::errors::DomainError;
use domain::value_objects::login::Login;
use serde::{Deserialize, Serialize};

use crate::router::{ApiError, AppState};

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

#[derive(Debug, Deserialize)]
pub struct SendAuthCodeRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SendAuthCodeResponse {
    pub email: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyAuthCodeRequest {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyAuthCodeResponse {
    pub valid: bool,
    pub user_id: Option<String>,
}

pub async fn login_handler(
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

pub async fn send_auth_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<SendAuthCodeRequest>,
) -> Result<Json<SendAuthCodeResponse>, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let one_time_code_service = state.one_time_code_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "One-time code service not initialized".to_string(),
    })?;

    let login = Login::from_email(&payload.email).map_err(ApiError::from_auth_domain)?;
    let user = user_repository
        .get_by_login(login.as_str())
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    one_time_code_service
        .issue(user.id, &payload.email)
        .await
        .map_err(ApiError::from_auth_domain)?;

    Ok(Json(SendAuthCodeResponse {
        email: payload.email.trim().to_ascii_lowercase(),
        message: "Code sent to email".to_string(),
    }))
}

pub async fn verify_auth_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<VerifyAuthCodeRequest>,
) -> Result<Json<VerifyAuthCodeResponse>, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let one_time_code_service = state.one_time_code_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "One-time code service not initialized".to_string(),
    })?;

    let login = Login::from_email(&payload.email).map_err(ApiError::from_auth_domain)?;
    let user = user_repository
        .get_by_login(login.as_str())
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    let valid = one_time_code_service
        .verify(user.id, &payload.code)
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !valid {
        return Err(ApiError::from_auth_domain(DomainError::InvalidAuthCode));
    }

    Ok(Json(VerifyAuthCodeResponse {
        valid: true,
        user_id: Some(user.id.to_string()),
    }))
}
