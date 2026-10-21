//! Plusnik routes: sheets, tasks, the teacher's matrix, pluses, the student dashboard.
//!
//! Access rules live in `PlusnikService`; handlers only parse input and map DTOs.
//! Dates: `issue_date` is `YYYY-MM-DD`, `deadline` is RFC 3339 (`2026-10-20T18:00:00+03:00`).

use std::str::FromStr;

use application::use_cases::plusnik::plusnik_service::{
    CreateSheetCommand, PlusnikService, SheetWithTasks, UpdateSheetCommand,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use domain::{
    entities::{
        plusnik::{PlusnikRecord, PlusnikSheet, PlusnikTask},
        user::User,
    },
    errors::DomainError,
    value_objects::sheet_status::SheetStatus,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Serialize)]
pub struct LessonResponse {
    pub lesson_id: String,
    pub subject_name: String,
    /// Class ("2027б") or group name.
    pub target_name: String,
}

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub number: String,
    pub sort_order: i32,
}

impl From<PlusnikTask> for TaskResponse {
    fn from(task: PlusnikTask) -> Self {
        Self {
            id: task.id.to_string(),
            number: task.task_number,
            sort_order: task.sort_order,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SheetResponse {
    pub id: String,
    pub lesson_id: String,
    pub name: String,
    /// `YYYY-MM-DD`.
    pub issue_date: String,
    /// RFC 3339 or null.
    pub deadline: Option<String>,
    /// draft | published | archived.
    pub status: String,
    pub created_by: String,
    pub tasks: Vec<TaskResponse>,
}

impl SheetResponse {
    fn new(sheet: PlusnikSheet, tasks: Vec<PlusnikTask>) -> Self {
        Self {
            id: sheet.id.to_string(),
            lesson_id: sheet.lesson_id.to_string(),
            name: sheet.name,
            issue_date: sheet.issue_date.to_string(),
            deadline: sheet.deadline.map(|deadline| deadline.to_rfc3339()),
            status: sheet.status.to_string(),
            created_by: sheet.created_by.to_string(),
            tasks: tasks.into_iter().map(TaskResponse::from).collect(),
        }
    }
}

impl From<SheetWithTasks> for SheetResponse {
    fn from(value: SheetWithTasks) -> Self {
        Self::new(value.sheet, value.tasks)
    }
}

#[derive(Debug, Serialize)]
pub struct PlusResponse {
    pub record_id: String,
    pub task_id: String,
    pub student_id: String,
    pub granted_by: String,
    pub granted_at: String,
}

impl From<PlusnikRecord> for PlusResponse {
    fn from(record: PlusnikRecord) -> Self {
        Self {
            record_id: record.id.to_string(),
            task_id: record.task_id.to_string(),
            student_id: record.student_id.to_string(),
            granted_by: record.granted_by.to_string(),
            granted_at: record.granted_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MatrixStudentResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct MatrixResponse {
    /// Sheet with tasks (columns of the matrix).
    pub sheet: SheetResponse,
    /// Students of the lesson (rows), sorted by name.
    pub students: Vec<MatrixStudentResponse>,
    /// Active pluses (filled cells).
    pub pluses: Vec<PlusResponse>,
}

#[derive(Debug, Serialize)]
pub struct GrantPlusResponse {
    pub plus: PlusResponse,
    /// False if the plus was already there.
    pub created: bool,
}

#[derive(Debug, Serialize)]
pub struct RevokePlusResponse {
    /// False if there was no active plus.
    pub revoked: bool,
}

#[derive(Debug, Serialize)]
pub struct SheetProgressResponse {
    pub sheet: SheetResponse,
    pub subject_name: String,
    pub total_tasks: usize,
    pub solved_tasks: usize,
    pub solved_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSheetRequest {
    pub name: String,
    pub issue_date: String,
    #[serde(default)]
    pub deadline: Option<String>,
    /// draft (default) | published | archived.
    #[serde(default)]
    pub status: Option<String>,
    /// Task numbers in display order: ["1а", "1б", "2"].
    #[serde(default)]
    pub tasks: Vec<String>,
}

/// Every field is optional: only the given ones change. `"deadline": null` clears it.
#[derive(Debug, Deserialize)]
pub struct UpdateSheetRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub issue_date: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub deadline: Option<Option<String>>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddTaskRequest {
    pub number: String,
}

#[derive(Debug, Deserialize)]
pub struct RevokePlusRequest {
    #[serde(default)]
    pub comment: Option<String>,
}

/// Tells a missing field (`None`) from an explicit `null` (`Some(None)`).
fn present_or_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

// ============================================================================
// Helpers
// ============================================================================

fn service(state: &AppState) -> Result<&PlusnikService, ApiError> {
    state.plusnik_service.as_deref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Plusnik service not initialized".to_string(),
    })
}

async fn caller<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
) -> Result<(User, &'a PlusnikService), ApiError> {
    let user = require_authenticated_user(state, headers).await?;
    Ok((user, service(state)?))
}

fn parse_id(raw: &str, not_found: DomainError) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::from_plusnik_domain(not_found))
}

fn bad_input(message: &str) -> ApiError {
    ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        message: message.to_string(),
    }
}

fn parse_date(raw: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| bad_input("issue_date must be YYYY-MM-DD"))
}

fn parse_deadline(raw: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|deadline| deadline.with_timezone(&Utc))
        .map_err(|_| bad_input("deadline must be RFC 3339, e.g. 2026-10-20T18:00:00+03:00"))
}

fn parse_status(raw: &str) -> Result<SheetStatus, ApiError> {
    SheetStatus::from_str(raw.trim()).map_err(ApiError::from_plusnik_domain)
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /api/plusnik/lessons` — lessons the teacher teaches; for an admin, all lessons.
pub async fn teacher_lessons_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LessonResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lessons = service
        .teacher_lessons(&user)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(
        lessons
            .into_iter()
            .map(|lesson| LessonResponse {
                lesson_id: lesson.lesson_id.to_string(),
                subject_name: lesson.subject_name,
                target_name: lesson.target_name,
            })
            .collect(),
    ))
}

/// `GET /api/plusnik/lessons/:lesson_id/sheets` — sheets with tasks, newest first.
pub async fn lesson_sheets_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<SheetResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let sheets = service
        .lesson_sheets(&user, lesson_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(sheets.into_iter().map(SheetResponse::from).collect()))
}

/// `POST /api/plusnik/lessons/:lesson_id/sheets` — creates a sheet with its tasks.
pub async fn create_sheet_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateSheetRequest>,
) -> Result<(StatusCode, Json<SheetResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let command = CreateSheetCommand {
        lesson_id: parse_id(&lesson_id, DomainError::LessonNotFound)?,
        name: payload.name,
        issue_date: parse_date(&payload.issue_date)?,
        deadline: payload.deadline.as_deref().map(parse_deadline).transpose()?,
        status: payload
            .status
            .as_deref()
            .map(parse_status)
            .transpose()?
            .unwrap_or(SheetStatus::Draft),
        task_numbers: payload.tasks,
    };
    let created = service
        .create_sheet(&user, command)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok((StatusCode::CREATED, Json(created.into())))
}

/// `PATCH /api/plusnik/sheets/:sheet_id` — rename, change dates, publish or archive.
pub async fn update_sheet_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSheetRequest>,
) -> Result<Json<SheetResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let command = UpdateSheetCommand {
        name: payload.name,
        issue_date: payload.issue_date.as_deref().map(parse_date).transpose()?,
        deadline: match payload.deadline {
            None => None,
            Some(None) => Some(None),
            Some(Some(raw)) => Some(Some(parse_deadline(&raw)?)),
        },
        status: payload.status.as_deref().map(parse_status).transpose()?,
    };
    let updated = service
        .update_sheet(&user, sheet_id, command)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(updated.into()))
}

/// `DELETE /api/plusnik/sheets/:sheet_id` — only sheets without pluses (409 otherwise).
pub async fn delete_sheet_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    service
        .delete_sheet(&user, sheet_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/plusnik/sheets/:sheet_id/matrix` — tasks × students with active pluses.
pub async fn sheet_matrix_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MatrixResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let matrix = service
        .sheet_matrix(&user, sheet_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(MatrixResponse {
        sheet: SheetResponse::new(matrix.sheet, matrix.tasks),
        students: matrix
            .students
            .into_iter()
            .map(|student| MatrixStudentResponse {
                id: student.id.to_string(),
                name: student.name,
            })
            .collect(),
        pluses: matrix.pluses.into_iter().map(PlusResponse::from).collect(),
    }))
}

/// `POST /api/plusnik/sheets/:sheet_id/tasks` — appends a task.
pub async fn add_task_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AddTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let task = service
        .add_task(&user, sheet_id, payload.number)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok((StatusCode::CREATED, Json(task.into())))
}

/// `DELETE /api/plusnik/tasks/:task_id` — only tasks without pluses (409 otherwise).
pub async fn delete_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let task_id = parse_id(&task_id, DomainError::PlusnikTaskNotFound)?;
    service
        .delete_task(&user, task_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/plusnik/tasks/:task_id/pluses/:student_id` — puts a plus (repeat-safe).
pub async fn grant_plus_handler(
    State(state): State<AppState>,
    Path((task_id, student_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<GrantPlusResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let task_id = parse_id(&task_id, DomainError::PlusnikTaskNotFound)?;
    let student_id = parse_id(&student_id, DomainError::UserNotFound)?;
    let granted = service
        .grant_plus(&user, task_id, student_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(GrantPlusResponse {
        plus: granted.record.into(),
        created: granted.created,
    }))
}

/// `DELETE /api/plusnik/tasks/:task_id/pluses/:student_id` — removes a plus,
/// optional body `{"comment": "..."}`. The record stays in history.
pub async fn revoke_plus_handler(
    State(state): State<AppState>,
    Path((task_id, student_id)): Path<(String, String)>,
    headers: HeaderMap,
    payload: Option<Json<RevokePlusRequest>>,
) -> Result<Json<RevokePlusResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let task_id = parse_id(&task_id, DomainError::PlusnikTaskNotFound)?;
    let student_id = parse_id(&student_id, DomainError::UserNotFound)?;
    let comment = payload.and_then(|Json(body)| body.comment);
    let revoked = service
        .revoke_plus(&user, task_id, student_id, comment)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(RevokePlusResponse { revoked }))
}

/// `GET /api/plusnik/me` — the student's dashboard: solved/total per published sheet.
pub async fn student_progress_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SheetProgressResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let progress = service
        .student_progress(&user)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(
        progress
            .into_iter()
            .map(|item| SheetProgressResponse {
                solved_tasks: item.solved_task_ids.len(),
                solved_task_ids: item.solved_task_ids.iter().map(Uuid::to_string).collect(),
                total_tasks: item.total_tasks,
                subject_name: item.subject_name,
                sheet: SheetResponse::new(item.sheet, Vec::new()),
            })
            .collect(),
    ))
}
