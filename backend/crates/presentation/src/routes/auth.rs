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
use domain::entities::user::User;
use domain::entities::web_session::{NewWebSession, WebSession};
use domain::errors::DomainError;
use domain::value_objects::login::Login;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::use_cases::auth::email_login::{EmailLoginService, LoginMethod};

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
    /// School email (`login` + the organization domain).
    pub email: String,
    pub role: String,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: Option<String>,
    pub is_active: bool,
    pub class_id: Option<String>,
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
            email: email_of(&session.user),
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

pub async fn init_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<InitSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let token_issuer = state.token_issuer.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Token issuer not initialized".to_string(),
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

    let (response_headers, created) = start_web_session(&state, &headers, &user, payload.remember_me).await?;

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
        email: email_of(&updated_user),
        login: updated_user.login,
        role: updated_user.role.to_string(),
        first_name: updated_user.first_name,
        last_name: updated_user.last_name,
        middle_name: updated_user.middle_name,
        is_active: updated_user.is_active,
        class_id: updated_user.class_id.map(|id| id.to_string()),
    }))
}

/// Creates a web session for `user` and the `Set-Cookie` header with its token.
///
/// The raw token goes only to the cookie (HttpOnly); the database keeps its HMAC.
/// `remember_me`: 30 days, otherwise 12 hours.
pub(crate) async fn start_web_session(
    state: &AppState,
    headers: &HeaderMap,
    user: &User,
    remember_me: bool,
) -> Result<(HeaderMap, WebSession), ApiError> {
    let not_ready = |what: &str| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("{what} not initialized"),
    };
    let web_session_repository = state
        .web_session_repository
        .as_ref()
        .ok_or_else(|| not_ready("Web session repository"))?;
    let session_token_hasher = state
        .session_token_hasher
        .as_ref()
        .ok_or_else(|| not_ready("Session token hasher"))?;
    let session_token_generator = state
        .session_token_generator
        .as_ref()
        .ok_or_else(|| not_ready("Session token generator"))?;
    let payload_remember_me = remember_me;

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

    let max_age_seconds: u64 = if payload_remember_me {
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
        payload_remember_me,
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

    Ok((response_headers, created))
}

/// School email of a user; the raw login if it is not a valid one (the agent's service user).
fn email_of(user: &User) -> String {
    Login::try_new(&user.login)
        .map(|login| login.email())
        .unwrap_or_else(|_| user.login.clone())
}

fn public_user(user: &User) -> PublicUser {
    PublicUser {
        id: user.id.to_string(),
        login: user.login.clone(),
        email: email_of(user),
        role: user.role.to_string(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        middle_name: user.middle_name.clone(),
        is_active: user.is_active,
        class_id: user.class_id.map(|id| id.to_string()),
    }
}

// ============================================================================
// Sign-in by school email: password, or emailed code + mandatory new password
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct EmailRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct EmailStartResponse {
    /// Normalized address.
    pub email: String,
    /// `password` — ask for the password; `code` — first login, ask for the emailed code.
    pub method: &'static str,
    /// For `code`: false if a code was sent less than a minute ago (that one is valid).
    pub code_sent: bool,
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct EmailCodeRequest {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct SetupTokenResponse {
    /// One-time, 15 minutes: pass it to `POST /api/auth/email/password`.
    pub setup_token: String,
}

#[derive(Debug, Deserialize)]
pub struct SetPasswordRequest {
    pub email: String,
    pub setup_token: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

#[derive(Debug, Deserialize)]
pub struct EmailLoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

#[derive(Debug, Serialize)]
pub struct EmailSessionResponse {
    pub user: PublicUser,
    pub expires_at: String,
}

fn email_login(state: &AppState) -> Result<&EmailLoginService, ApiError> {
    state.email_login.as_deref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Email login not initialized".to_string(),
    })
}

/// Errors of the email sign-in, in Russian: the site shows them in the form as is.
fn email_auth_error(error: DomainError) -> ApiError {
    let (status, message) = match error {
        DomainError::InvalidEmailFormat => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Введите школьную почту (@179.ru)",
        ),
        DomainError::UserNotFound => (StatusCode::NOT_FOUND, "Аккаунта с такой почтой нет. Обратитесь к администратору школы"),
        DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "Аккаунт отключён"),
        DomainError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Неверная почта или пароль"),
        DomainError::InvalidAuthCode => (
            StatusCode::UNAUTHORIZED,
            "Неверный или устаревший код. После 5 ошибок запросите новый",
        ),
        DomainError::InvalidPasswordFormat => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Пароль должен быть от 8 до 128 символов",
        ),
        DomainError::PasswordAlreadySet => (
            StatusCode::CONFLICT,
            "Пароль уже задан — войдите с ним",
        ),
        DomainError::PasswordNotSet => (
            StatusCode::CONFLICT,
            "Первый вход: подтвердите почту кодом и задайте пароль",
        ),
        DomainError::CodeDeliveryFailed => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Не удалось отправить письмо с кодом. Попробуйте позже или обратитесь к администратору",
        ),
        DomainError::AuthCodeCooldown => (
            StatusCode::TOO_MANY_REQUESTS,
            "Код уже отправлен. Новый можно запросить через минуту",
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка сервера. Попробуйте позже"),
    };
    ApiError {
        status,
        message: message.to_string(),
    }
}

/// `POST /api/auth/email/start` `{email}` — what to ask next: the password, or (first
/// login) the code that was just emailed.
pub async fn email_start_handler(
    State(state): State<AppState>,
    Json(payload): Json<EmailRequest>,
) -> Result<Json<EmailStartResponse>, ApiError> {
    let started = email_login(&state)?
        .start(&payload.email)
        .await
        .map_err(email_auth_error)?;
    let email = Login::from_email(&payload.email)
        .map(|login| login.email())
        .map_err(email_auth_error)?;
    Ok(Json(EmailStartResponse {
        email,
        method: match started.method {
            LoginMethod::Password => "password",
            LoginMethod::Code => "code",
        },
        code_sent: started.code_sent,
    }))
}

/// `POST /api/auth/email/code/resend` `{email}` — a new code (once a minute, 429 otherwise).
pub async fn email_resend_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<EmailRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    email_login(&state)?
        .resend_code(&payload.email)
        .await
        .map_err(email_auth_error)?;
    Ok(Json(OkResponse { ok: true }))
}

/// `POST /api/auth/email/code/verify` `{email, code}` — the code is right: a one-time
/// token for choosing the password. Does not log in.
pub async fn email_verify_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<EmailCodeRequest>,
) -> Result<Json<SetupTokenResponse>, ApiError> {
    let setup_token = email_login(&state)?
        .verify_code(&payload.email, &payload.code)
        .await
        .map_err(email_auth_error)?;
    Ok(Json(SetupTokenResponse { setup_token }))
}

/// `POST /api/auth/email/password` `{email, setup_token, password, remember_me?}` — saves
/// the first password and logs in (session cookie).
pub async fn email_set_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SetPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = email_login(&state)?
        .complete_first_login(&payload.email, &payload.setup_token, &payload.password)
        .await
        .map_err(email_auth_error)?;
    let (cookie, session) = start_web_session(&state, &headers, &user, payload.remember_me).await?;
    Ok((
        cookie,
        Json(EmailSessionResponse {
            user: public_user(&user),
            expires_at: session.expires_at.to_rfc3339(),
        }),
    ))
}

/// `POST /api/auth/email/login` `{email, password, remember_me?}` — logs in (session
/// cookie). 409 if the first login (code + new password) is not done yet.
pub async fn email_login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EmailLoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = email_login(&state)?
        .login(&payload.email, &payload.password)
        .await
        .map_err(email_auth_error)?;
    let (cookie, session) = start_web_session(&state, &headers, &user, payload.remember_me).await?;
    Ok((
        cookie,
        Json(EmailSessionResponse {
            user: public_user(&user),
            expires_at: session.expires_at.to_rfc3339(),
        }),
    ))
}
