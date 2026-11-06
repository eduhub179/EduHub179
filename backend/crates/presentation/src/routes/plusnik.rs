//! Plusnik routes: sheets, tasks, the teacher's matrix, pluses, the student dashboard.
//!
//! Access rules live in `PlusnikService`; handlers only parse input and map DTOs.
//! Dates: `issue_date` is `YYYY-MM-DD`, `deadline` is RFC 3339 (`2026-10-20T18:00:00+03:00`).

use std::str::FromStr;

use application::use_cases::plusnik::plusnik_service::{
    CourseSummary, CreateSheetCommand, HistoryFilter, PlusBatchCommand, PlusCell,
    PlusHistoryEntry, PlusnikService, SheetWithTasks, UpdateSheetCommand,
};
use axum::{
    extract::{Path, Query, State},
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
    /// 0-based place among the existing tasks; absent or past the end = append.
    #[serde(default)]
    pub position: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AddTasksRequest {
    /// Task numbers in display order: ["1а", "1б", "2"].
    pub numbers: Vec<String>,
    /// 0-based place among the existing tasks; absent or past the end = append.
    #[serde(default)]
    pub position: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct RenameTaskRequest {
    pub number: String,
}

#[derive(Debug, Deserialize)]
pub struct TaskIdsRequest {
    pub task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlusCellRequest {
    pub task_id: String,
    pub student_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PlusBatchRequest {
    #[serde(default)]
    pub grant: Vec<PlusCellRequest>,
    #[serde(default)]
    pub revoke: Vec<PlusCellRequest>,
    /// Stored on every revoked record.
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlusBatchResponse {
    /// Pluses actually added (present ones are skipped).
    pub granted: usize,
    /// Pluses actually removed (absent ones are skipped).
    pub revoked: usize,
    /// All active pluses of the sheet after the change.
    pub pluses: Vec<PlusResponse>,
}

#[derive(Debug, Serialize)]
pub struct SummarySheetResponse {
    pub id: String,
    pub name: String,
    pub issue_date: String,
    pub deadline: Option<String>,
    /// published | archived.
    pub status: String,
    pub total_tasks: usize,
}

#[derive(Debug, Serialize)]
pub struct SummaryStudentResponse {
    pub id: String,
    pub name: String,
    /// Solved tasks per sheet, aligned with `sheets`.
    pub solved: Vec<usize>,
    pub solved_total: usize,
}

#[derive(Debug, Serialize)]
pub struct CourseSummaryResponse {
    pub lesson: LessonResponse,
    /// Published and archived sheets, oldest first.
    pub sheets: Vec<SummarySheetResponse>,
    /// Sorted by name.
    pub students: Vec<SummaryStudentResponse>,
    pub total_tasks: usize,
}

impl From<CourseSummary> for CourseSummaryResponse {
    fn from(summary: CourseSummary) -> Self {
        Self {
            lesson: LessonResponse {
                lesson_id: summary.lesson.lesson_id.to_string(),
                subject_name: summary.lesson.subject_name,
                target_name: summary.lesson.target_name,
            },
            sheets: summary
                .sheets
                .into_iter()
                .map(|item| SummarySheetResponse {
                    id: item.sheet.id.to_string(),
                    name: item.sheet.name,
                    issue_date: item.sheet.issue_date.to_string(),
                    deadline: item.sheet.deadline.map(|deadline| deadline.to_rfc3339()),
                    status: item.sheet.status.to_string(),
                    total_tasks: item.total_tasks,
                })
                .collect(),
            students: summary
                .students
                .into_iter()
                .map(|student| SummaryStudentResponse {
                    id: student.id.to_string(),
                    name: student.name,
                    solved: student.solved,
                    solved_total: student.solved_total,
                })
                .collect(),
            total_tasks: summary.total_tasks,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryEntryResponse {
    pub record_id: String,
    pub task_id: String,
    pub task_number: String,
    pub student_id: String,
    pub student_name: String,
    pub granted_by: String,
    pub granted_by_name: String,
    pub granted_at: String,
    /// Null while the plus is active.
    pub revoked_at: Option<String>,
    pub revoked_by: Option<String>,
    pub revoked_by_name: Option<String>,
    pub revoke_comment: Option<String>,
}

impl From<PlusHistoryEntry> for HistoryEntryResponse {
    fn from(entry: PlusHistoryEntry) -> Self {
        let record = entry.record;
        Self {
            record_id: record.id.to_string(),
            task_id: record.task_id.to_string(),
            task_number: entry.task_number,
            student_id: record.student_id.to_string(),
            student_name: entry.student_name,
            granted_by: record.granted_by.to_string(),
            granted_by_name: entry.granted_by_name,
            granted_at: record.granted_at.to_rfc3339(),
            revoked_at: record.revoked_at.map(|at| at.to_rfc3339()),
            revoked_by: record.revoked_by.map(|id| id.to_string()),
            revoked_by_name: entry.revoked_by_name,
            revoke_comment: record.revoke_comment,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub student_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
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

fn parse_task_ids(raw: &[String]) -> Result<Vec<Uuid>, ApiError> {
    raw.iter()
        .map(|id| parse_id(id.trim(), DomainError::PlusnikTaskNotFound))
        .collect()
}

fn parse_cells(raw: Vec<PlusCellRequest>) -> Result<Vec<PlusCell>, ApiError> {
    raw.into_iter()
        .map(|cell| {
            Ok(PlusCell {
                task_id: parse_id(cell.task_id.trim(), DomainError::PlusnikTaskNotFound)?,
                student_id: parse_id(cell.student_id.trim(), DomainError::UserNotFound)?,
            })
        })
        .collect()
}

/// Empty or missing = no filter.
fn optional_id(raw: Option<&str>, not_found: DomainError) -> Result<Option<Uuid>, ApiError> {
    raw.map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| parse_id(raw, not_found))
        .transpose()
}

fn parse_status(raw: &str) -> Result<SheetStatus, ApiError> {
    SheetStatus::from_str(raw.trim()).map_err(ApiError::from_plusnik_domain)
}

// ============================================================================
// Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LessonsQuery {
    /// Only lessons of this class (its whole-class and group lessons).
    #[serde(default)]
    pub class_id: Option<String>,
}

/// `GET /api/plusnik/lessons[?class_id=<uuid>]` — lessons the teacher teaches; for an
/// admin, all lessons. With `class_id`, only lessons of that class, group lessons included.
pub async fn teacher_lessons_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LessonsQuery>,
) -> Result<Json<Vec<LessonResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let class_id = query
        .class_id
        .as_deref()
        .filter(|raw| !raw.trim().is_empty())
        .map(|raw| parse_id(raw.trim(), DomainError::ClassNotFound))
        .transpose()?;
    let lessons = service
        .teacher_lessons(&user, class_id)
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

/// `POST /api/plusnik/sheets/:sheet_id/tasks` — adds a task: `{number, position?}`.
/// Without `position` it goes to the end; tasks after it shift down.
pub async fn add_task_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AddTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let task = service
        .insert_task(&user, sheet_id, payload.number, payload.position)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok((StatusCode::CREATED, Json(task.into())))
}

/// `POST /api/plusnik/sheets/:sheet_id/tasks/batch` — adds several tasks at once:
/// `{numbers: ["5а","5б"], position?}`. All or nothing; returns all tasks of the sheet.
pub async fn add_tasks_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AddTasksRequest>,
) -> Result<(StatusCode, Json<Vec<TaskResponse>>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let added = service
        .add_tasks(&user, sheet_id, payload.numbers, payload.position)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok((
        StatusCode::CREATED,
        Json(added.tasks.into_iter().map(TaskResponse::from).collect()),
    ))
}

/// `DELETE /api/plusnik/sheets/:sheet_id/tasks` — deletes several tasks:
/// `{task_ids: [...]}`. All or nothing (409 if any has pluses); returns the rest.
pub async fn delete_tasks_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<TaskIdsRequest>,
) -> Result<Json<Vec<TaskResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let task_ids = parse_task_ids(&payload.task_ids)?;
    let tasks = service
        .delete_tasks(&user, sheet_id, task_ids)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(tasks.into_iter().map(TaskResponse::from).collect()))
}

/// `PUT /api/plusnik/sheets/:sheet_id/tasks/order` — new order of all tasks:
/// `{task_ids: [...]}` listing every task once.
pub async fn reorder_tasks_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<TaskIdsRequest>,
) -> Result<Json<Vec<TaskResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let task_ids = parse_task_ids(&payload.task_ids)?;
    let tasks = service
        .reorder_tasks(&user, sheet_id, task_ids)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(tasks.into_iter().map(TaskResponse::from).collect()))
}

/// `PATCH /api/plusnik/tasks/:task_id` — renames a task: `{number}`. Pluses stay.
pub async fn rename_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<RenameTaskRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let task_id = parse_id(&task_id, DomainError::PlusnikTaskNotFound)?;
    let task = service
        .rename_task(&user, task_id, payload.number)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(task.into()))
}

/// `POST /api/plusnik/sheets/:sheet_id/pluses` — many pluses at once:
/// `{grant: [{task_id, student_id}], revoke: [...], comment?}`. One transaction,
/// repeat-safe; returns counts and the sheet's active pluses.
pub async fn apply_pluses_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<PlusBatchRequest>,
) -> Result<Json<PlusBatchResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let command = PlusBatchCommand {
        grant: parse_cells(payload.grant)?,
        revoke: parse_cells(payload.revoke)?,
        comment: payload.comment,
    };
    let result = service
        .apply_pluses(&user, sheet_id, command)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(PlusBatchResponse {
        granted: result.granted,
        revoked: result.revoked,
        pluses: result.pluses.into_iter().map(PlusResponse::from).collect(),
    }))
}

/// `GET /api/plusnik/lessons/:lesson_id/summary` — students × sheets with solved
/// counts over published and archived sheets (teachers and admins).
pub async fn course_summary_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CourseSummaryResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let summary = service
        .course_summary(&user, lesson_id)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(summary.into()))
}

/// `GET /api/plusnik/sheets/:sheet_id/history[?student_id=&task_id=]` — every plus
/// ever put on the sheet, revoked included, newest first, with names.
pub async fn sheet_history_handler(
    State(state): State<AppState>,
    Path(sheet_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let sheet_id = parse_id(&sheet_id, DomainError::PlusnikSheetNotFound)?;
    let filter = HistoryFilter {
        student_id: optional_id(query.student_id.as_deref(), DomainError::UserNotFound)?,
        task_id: optional_id(query.task_id.as_deref(), DomainError::PlusnikTaskNotFound)?,
    };
    let history = service
        .sheet_history(&user, sheet_id, filter)
        .await
        .map_err(ApiError::from_plusnik_domain)?;

    Ok(Json(history.into_iter().map(HistoryEntryResponse::from).collect()))
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
