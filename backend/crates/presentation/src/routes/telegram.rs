use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use domain::errors::DomainError;
use serde::Serialize;

use crate::router::{ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct TelegramUserResponse {
    pub user_id: String,
    pub login: String,
    pub role: String,
    pub class_id: Option<String>,
}

pub async fn telegram_user_handler(
    State(state): State<AppState>,
    Path(telegram_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Json<TelegramUserResponse>, ApiError> {
    let not_found = || ApiError {
        status: StatusCode::NOT_FOUND,
        message: "Telegram account is not linked".to_string(),
    };
    let lookup = state.telegram_lookup.as_ref().ok_or_else(not_found)?;
    let key = headers.get("x-agent-key").and_then(|v| v.to_str().ok()).unwrap_or("");
    if lookup.agent_key.is_empty() || key != lookup.agent_key {
        return Err(ApiError {
            status: StatusCode::UNAUTHORIZED,
            message: "Agent key required".to_string(),
        });
    }

    let user_id = match lookup.bindings.get_active_user_by_telegram_id(telegram_id).await {
        Ok(id) => id,
        Err(DomainError::TelegramBindingNotFound) => return Err(not_found()),
        Err(e) => return Err(ApiError::from_homework_domain(e)),
    };
    let user = match lookup.users.get_by_id(user_id).await {
        Ok(user) if user.is_active => user,
        Ok(_) | Err(DomainError::UserNotFound) => return Err(not_found()),
        Err(e) => return Err(ApiError::from_homework_domain(e)),
    };

    Ok(Json(TelegramUserResponse {
        user_id: user.id.to_string(),
        login: user.login,
        role: user.role.to_string(),
        class_id: user.class_id.map(|id| id.to_string()),
    }))
}
