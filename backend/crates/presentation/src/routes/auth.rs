use application::use_cases::auth::login::LoginCommand;
use axum::{
    extract::State,
    http::{
        header::{SET_COOKIE, USER_AGENT},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use domain::entities::web_session::NewWebSession;
use domain::errors::DomainError;
use domain::value_objects::login::Login;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};

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

#[derive(Debug, Deserialize)]
pub struct InitSessionRequest {
    pub user_id: String,
    pub auth_token: String,
    #[serde(default)]
    pub remember_me: bool,
}

#[derive(Debug, Serialize)]
pub struct InitSessionResponse {
    pub ok: bool,
    pub user_id: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileClassRequest {
    pub class_id: String,
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

pub async fn init_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<InitSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
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
    let token_issuer = state.token_issuer.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Token issuer not initialized".to_string(),
    })?;
    let session_token_generator =
        state
            .session_token_generator
            .as_ref()
            .ok_or_else(|| ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Session token generator not initialized".to_string(),
            })?;

    let user_id = Uuid::parse_str(&payload.user_id)
        .map_err(|_| ApiError::from_auth_domain(DomainError::UserNotFound))?;
    let claims = token_issuer
        .verify(&payload.auth_token)
        .map_err(ApiError::from_auth_domain)?;
    if claims.user_id != user_id {
        return Err(ApiError::from_auth_domain(DomainError::InvalidCredentials));
    }
    let user = user_repository
        .get_by_id(user_id)
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    let raw_session_token = session_token_generator.generate_session_token();
    let session_token_hash = session_token_hasher
        .hash_session_token(&raw_session_token)
        .map_err(ApiError::from_auth_domain)?;

    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let max_age_seconds: u64 = if payload.remember_me {
        60 * 60 * 24 * 30
    } else {
        60 * 60 * 12
    };
    let expires_at = Utc::now() + Duration::seconds(max_age_seconds as i64);

    let session = NewWebSession::try_new(
        user.id,
        session_token_hash,
        None,
        ip_address,
        user_agent,
        payload.remember_me,
        expires_at,
    )
    .map_err(ApiError::from_auth_domain)?;

    let created = web_session_repository
        .create(session)
        .await
        .map_err(ApiError::from_auth_domain)?;

    let secure_attr = if cfg!(debug_assertions) { "" } else { "; Secure" };
    let cookie_value = format!(
        "session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        raw_session_token, max_age_seconds, secure_attr
    );

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_value).map_err(|_| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to build session cookie".to_string(),
        })?,
    );

    Ok((
        response_headers,
        Json(InitSessionResponse {
            ok: true,
            user_id: created.user_id.to_string(),
            expires_at: created.expires_at.to_rfc3339(),
        }),
    ))
}

pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let web_session_repository = state
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

    let raw_token = headers
        .get(axum::http::header::COOKIE)
        .and_then(|header| header.to_str().ok())
        .and_then(|cookie_header| {
            cookie_header.split(';').find_map(|pair| {
                let mut parts = pair.trim().splitn(2, '=');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                (name == "session").then(|| value.to_string())
            })
        });

    if let Some(raw_token) = raw_token {
        if let Ok(token_hash) = session_token_hasher.hash_session_token(&raw_token) {
            if let Ok(session) = web_session_repository.get_active_by_token_hash(&token_hash).await {
                let _ = web_session_repository
                    .revoke(session.session_id, Some("user_logout".to_string()))
                    .await;
            }
        }
    }

    let secure_attr = if cfg!(debug_assertions) { "" } else { "; Secure" };
    let clear_cookie_value = format!(
        "session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
        secure_attr
    );

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&clear_cookie_value).map_err(|_| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to clear session cookie".to_string(),
        })?,
    );

    Ok((response_headers, Json(LogoutResponse { ok: true })))
}

pub async fn update_profile_class_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProfileClassRequest>,
) -> Result<Json<PublicUser>, ApiError> {
    let auth_user = require_authenticated_user(&state, &headers).await?;
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let class_repository = state.class_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Class repository not initialized".to_string(),
    })?;

    let class_id = Uuid::parse_str(&payload.class_id).map_err(|_| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "Invalid class id".to_string(),
    })?;

    let class = class_repository.get_by_id(class_id).await.map_err(|_| ApiError {
        status: StatusCode::NOT_FOUND,
        message: "Class not found".to_string(),
    })?;
    if !class.is_active {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "Class not found".to_string(),
        });
    }

    let mut current_user = user_repository
        .get_by_id(auth_user.id)
        .await
        .map_err(ApiError::from_auth_domain)?;
    current_user.class_id = Some(class_id);
    let updated_user = user_repository
        .save(current_user)
        .await
        .map_err(ApiError::from_auth_domain)?;

    Ok(Json(PublicUser {
        id: updated_user.id.to_string(),
        login: updated_user.login,
        role: updated_user.role.to_string(),
        first_name: updated_user.first_name,
        last_name: updated_user.last_name,
        middle_name: updated_user.middle_name,
        is_active: updated_user.is_active,
        class_id: updated_user.class_id.map(|id| id.to_string()),
    }))
}
