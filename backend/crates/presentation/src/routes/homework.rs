use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use domain::{errors::DomainError, value_objects::homework_status::HomeworkStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::use_cases::notifications::notify_homework_changed::HomeworkChange;

use crate::router::{can_edit_homework, require_caller, ApiError, AppState, Caller};
use crate::routes::notifications::{notify_homework_change_for_lesson, notify_homework_changed};

/// The author is always the authenticated caller (or `X-Agent-On-Behalf-Of` for the agent),
/// never taken from the body: older clients may still send `created_by` / `created_by_role`,
/// unknown fields are ignored.
#[derive(Debug, Deserialize)]
pub struct CreateHomeworkRequest {
    pub lesson_instance_id: String,
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
    let Caller { user, is_agent } = require_caller(&state, &headers).await?;
    // The agent may write homework on behalf of any user, e.g. a student who sent it to the bot in DM.
    if !can_edit_homework(&user, is_agent) {
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

    notify_homework_changed(&state, homework.id, HomeworkChange::Created, user.id);

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
    let Caller { user, is_agent } = require_caller(&state, &headers).await?;
    if !can_edit_homework(&user, is_agent) {
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

    notify_homework_changed(&state, homework.id, HomeworkChange::TextUpdated, user.id);

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

#[derive(Debug, Default, Deserialize)]
pub struct DeleteHomeworkQuery {
    /// Also delete the homework of the other lessons of a double lesson (the site does;
    /// the agent does not: it tracks both copies itself).
    #[serde(default)]
    pub with_double: bool,
}

/// `DELETE /api/homework/:homework_id[?with_double=true]` — removes the homework with its
/// text and all files.
pub async fn delete_homework_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(homework_id): Path<String>,
    Query(query): Query<DeleteHomeworkQuery>,
) -> Result<StatusCode, ApiError> {
    let Caller { user, is_agent } = require_caller(&state, &headers).await?;
    if !can_edit_homework(&user, is_agent) {
        return Err(ApiError::from_homework_domain(DomainError::InsufficientPermissions));
    }

    let delete_homework_use_case = state.delete_homework_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Delete homework use case not initialized".to_string(),
    })?;

    let homework_uuid = Uuid::parse_str(&homework_id)
        .map_err(|_| ApiError::from_homework_domain(DomainError::HomeworkNotFound))?;

    let deleted = delete_homework_use_case
        .execute(homework_uuid)
        .await
        .map_err(ApiError::from_homework_domain)?;
    let mut removed = vec![(deleted, homework_uuid)];

    if query.with_double {
        // Best effort: the homework asked for is already gone, a failure here leaves the
        // partner's copy in place and is only logged.
        let partners = match &state.schedule_service {
            Some(schedule) => schedule
                .double_lesson_partners(removed[0].0.lesson_instance_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("double lesson of homework {homework_uuid}: {error}");
                    Vec::new()
                }),
            None => Vec::new(),
        };
        for partner in partners {
            let Some(details) = partner.homework else { continue };
            let partner_id = details.homework.id;
            match delete_homework_use_case.execute(partner_id).await {
                Ok(deleted) => removed.push((deleted, partner_id)),
                Err(error) => eprintln!("double lesson: could not delete homework {partner_id}: {error}"),
            }
        }
    }

    for (deleted, id) in removed {
        for key in deleted.orphaned_storage_keys {
            // The homework is gone, the object stayed: leave a trace for manual cleanup.
            eprintln!("file storage: could not delete object '{key}' (homework {id} removed)");
        }
        notify_homework_change_for_lesson(
            &state,
            deleted.lesson_instance_id,
            id,
            HomeworkChange::Deleted,
            user.id,
        );
    }

    Ok(StatusCode::NO_CONTENT)
}
