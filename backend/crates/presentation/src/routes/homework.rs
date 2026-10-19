use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use domain::{errors::DomainError, value_objects::homework_status::HomeworkStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct CreateHomeworkRequest {
    pub lesson_instance_id: String,
    pub created_by: String,
    #[serde(default)]
    pub created_by_role: String,
    #[serde(default)]
    pub text_content: Option<String>,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateHomeworkResponse {
    pub id: String,
    pub lesson_instance_id: String,
    pub created_by: String,
    pub created_by_role: String,
    pub text_content: Option<String>,
    pub status: String,
    pub locked_by_teacher: bool,
    pub last_edited_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHomeworkRequest {
    pub text_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateHomeworkResponse {
    pub id: String,
    pub lesson_instance_id: String,
    pub created_by: String,
    pub created_by_role: String,
    pub text_content: Option<String>,
    pub status: String,
    pub locked_by_teacher: bool,
    pub last_edited_by: Option<String>,
    pub created_at: String,
}

pub async fn create_homework_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateHomeworkRequest>,
) -> Result<Json<CreateHomeworkResponse>, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    if !user.is_teacher() {
        return Err(ApiError::from_homework_domain(DomainError::InsufficientPermissions));
    }

    let create_homework_use_case = state.create_homework_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Create homework use case not initialized".to_string(),
    })?;

    let lesson_instance_id = Uuid::parse_str(&payload.lesson_instance_id)
        .map_err(|_| ApiError::from_homework_domain(DomainError::LessonInstanceNotFound))?;
    let status = HomeworkStatus::from_str(&payload.status)
        .map_err(ApiError::from_homework_domain)?;

    let homework = create_homework_use_case
        .execute(application::use_cases::homework::create_homework::CreateHomeworkCommand::new(
            lesson_instance_id,
            user.id,
            user.role,
            payload.text_content,
            status,
        ))
        .await
        .map_err(ApiError::from_homework_domain)?;

    Ok(Json(CreateHomeworkResponse {
        id: homework.id.to_string(),
        lesson_instance_id: homework.lesson_instance_id.to_string(),
        created_by: homework.created_by.to_string(),
        created_by_role: homework.created_by_role.to_string(),
        text_content: homework.text_content,
        status: homework.status.to_string(),
        locked_by_teacher: homework.locked_by_teacher,
        last_edited_by: homework.last_edited_by.map(|id| id.to_string()),
        created_at: homework.created_at.to_rfc3339(),
    }))
}

pub async fn update_homework_text_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(homework_id): Path<String>,
    Json(payload): Json<UpdateHomeworkRequest>,
) -> Result<Json<UpdateHomeworkResponse>, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    if !user.is_teacher() {
        return Err(ApiError::from_homework_domain(DomainError::InsufficientPermissions));
    }

    let update_homework_use_case = state.update_homework_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Update homework use case not initialized".to_string(),
    })?;

    let homework_uuid = Uuid::parse_str(&homework_id)
        .map_err(|_| ApiError::from_homework_domain(DomainError::HomeworkNotFound))?;

    let homework = update_homework_use_case
        .execute(application::use_cases::homework::update_homework_text::UpdateHomeworkTextCommand::new(homework_uuid, payload.text_content))
        .await
        .map_err(ApiError::from_homework_domain)?;

    Ok(Json(UpdateHomeworkResponse {
        id: homework.id.to_string(),
        lesson_instance_id: homework.lesson_instance_id.to_string(),
        created_by: homework.created_by.to_string(),
        created_by_role: homework.created_by_role.to_string(),
        text_content: homework.text_content,
        status: homework.status.to_string(),
        locked_by_teacher: homework.locked_by_teacher,
        last_edited_by: homework.last_edited_by.map(|id| id.to_string()),
        created_at: homework.created_at.to_rfc3339(),
    }))
}
