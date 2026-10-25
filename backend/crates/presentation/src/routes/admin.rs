//! Admin panel routes (`/api/admin/*`): users, lessons of a day, schedule weeks,
//! announcements. Plus `GET /api/announcements` for every logged-in user.
//!
//! Access: a web session of an active admin. The chat agent (`X-Agent-Key`) is
//! refused even when it acts on behalf of an admin: admin actions are made by
//! people in the panel only. `AdminService` checks the role again.
//!
//! Dates: `YYYY-MM-DD`; `expires_at` is RFC 3339.

use std::str::FromStr;

use application::use_cases::admin::{
    AdminService, AnnouncementInput, LessonChange, NewUser, UserChange, WeekSource,
};
use application::use_cases::schedule::schedule::ScheduleLesson;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use domain::{
    entities::{announcement::Announcement, schedule_week::ScheduleWeek, user::User},
    errors::DomainError,
    repositories::admin_repository::{AdminAnnouncementRow, AdminUserRow, UserFilter},
    value_objects::{
        announcement_priority::AnnouncementPriority, announcement_status::AnnouncementStatus,
        lesson_instance_status::LessonInstanceStatus, role::UserRole,
    },
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, require_caller, ApiError, AppState};

// ============================================================================
// Errors
// ============================================================================

impl ApiError {
    pub(crate) fn from_admin_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
            DomainError::ClassNotFound => (StatusCode::NOT_FOUND, "Class not found"),
            DomainError::CabinetNotFound => (StatusCode::NOT_FOUND, "Cabinet not found"),
            DomainError::LessonInstanceNotFound => (StatusCode::NOT_FOUND, "Lesson not found"),
            DomainError::ScheduleWeekNotFound => (StatusCode::NOT_FOUND, "Schedule week not found"),
            DomainError::AnnouncementNotFound => (StatusCode::NOT_FOUND, "Announcement not found"),
            DomainError::LoginAlreadyExists => (StatusCode::CONFLICT, "Login already exists"),
            DomainError::CabinetAlreadyExists => (StatusCode::CONFLICT, "Cabinet already exists"),
            DomainError::InvalidCabinetNumber => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Cabinet number must be 100-999")
            }
            DomainError::InvalidCabinetDescription => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Cabinet description is too long")
            }
            DomainError::ScheduleWeekAlreadyExists => {
                (StatusCode::CONFLICT, "Schedule week already exists")
            }
            DomainError::LessonInstanceAlreadyExists => {
                (StatusCode::CONFLICT, "The week already has lessons")
            }
            DomainError::InvalidLoginFormat => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Login: latin letters, digits, '.', '_', '-' (the part of the email before @)",
            ),
            DomainError::InvalidNameFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Last and first name are required")
            }
            DomainError::InvalidLessonInstanceDate => {
                (StatusCode::UNPROCESSABLE_ENTITY, "A week must start on a Monday")
            }
            DomainError::InvalidSubstitutionNote => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Note must be at most 500 characters")
            }
            DomainError::InvalidAnnouncementTitle => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Title must be at most 255 characters")
            }
            DomainError::InvalidAnnouncementContent => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Announcement needs a title or text")
            }
            DomainError::UserIsInactive => (StatusCode::CONFLICT, "User account is inactive"),
            DomainError::InvalidPasswordFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Password must be 8-128 characters")
            }
            DomainError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            DomainError::InternalError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };
        Self {
            status,
            message: message.to_string(),
        }
    }
}

fn bad_request(message: &str) -> ApiError {
    ApiError {
        status: StatusCode::BAD_REQUEST,
        message: message.to_string(),
    }
}

fn parse_uuid(raw: &str, what: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw.trim()).map_err(|_| bad_request(&format!("Invalid {what}")))
}

fn parse_date(raw: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").map_err(|_| bad_request("Invalid date, expected YYYY-MM-DD"))
}

/// Distinguishes a missing field (`None`) from an explicit `null` (`Some(None)`).
fn present_or_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn service(state: &AppState) -> Result<&AdminService, ApiError> {
    state.admin_service.as_deref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Admin service not initialized".to_string(),
    })
}

/// The admin making the request; the agent and non-admins get 403.
async fn admin<'a>(state: &'a AppState, headers: &HeaderMap) -> Result<(User, &'a AdminService), ApiError> {
    let service = service(state)?;
    let caller = require_caller(state, headers).await?;
    if caller.is_agent || !caller.user.is_admin() {
        return Err(ApiError::from_admin_domain(DomainError::InsufficientPermissions));
    }
    Ok((caller.user, service))
}

fn full_name(user: &User) -> String {
    format!("{} {}", user.last_name, user.first_name)
}

// ============================================================================
// Users
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AdminUserResponse {
    pub id: String,
    pub login: String,
    /// student | teacher | admin.
    pub role: String,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub is_active: bool,
    pub class_id: Option<String>,
    /// "2027б".
    pub class_name: Option<String>,
    pub telegram_bound: bool,
}

impl From<AdminUserRow> for AdminUserResponse {
    fn from(row: AdminUserRow) -> Self {
        let user = row.user;
        Self {
            id: user.id.to_string(),
            login: user.login,
            role: user.role.to_string(),
            last_name: user.last_name,
            first_name: user.first_name,
            middle_name: user.middle_name,
            is_active: user.is_active,
            class_id: user.class_id.map(|id| id.to_string()),
            class_name: row.class_name,
            telegram_bound: row.telegram_bound,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserResponse>,
    /// True when the list hit the limit and the search should be narrowed.
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct UserListQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub class_id: Option<String>,
    #[serde(default)]
    pub include_inactive: Option<bool>,
}

fn parse_role(raw: &str) -> Result<UserRole, ApiError> {
    UserRole::from_str(raw).map_err(|_| bad_request("Role must be student, teacher or admin"))
}

pub async fn list_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UserListQuery>,
) -> Result<Json<AdminUserListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let filter = UserFilter {
        query: query.q.filter(|q| !q.trim().is_empty()),
        role: query.role.filter(|r| !r.is_empty()).map(|r| parse_role(&r)).transpose()?,
        class_id: query
            .class_id
            .filter(|c| !c.is_empty())
            .map(|c| parse_uuid(&c, "class_id"))
            .transpose()?,
        include_inactive: query.include_inactive.unwrap_or(false),
    };
    let users = service
        .list_users(&actor, filter)
        .await
        .map_err(ApiError::from_admin_domain)?;
    let truncated = users.len() as i64
        >= application::use_cases::admin::admin_service::USER_LIST_LIMIT;
    Ok(Json(AdminUserListResponse {
        users: users.into_iter().map(AdminUserResponse::from).collect(),
        truncated,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub login: String,
    pub role: String,
    pub last_name: String,
    pub first_name: String,
    #[serde(default)]
    pub middle_name: Option<String>,
    #[serde(default)]
    pub class_id: Option<String>,
}

/// Short user data after create/update; the panel reloads the list for the rest.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub login: String,
    pub role: String,
    pub is_active: bool,
    pub class_id: Option<String>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            login: user.login,
            role: user.role.to_string(),
            is_active: user.is_active,
            class_id: user.class_id.map(|id| id.to_string()),
        }
    }
}

pub async fn create_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let new_user = NewUser {
        login: request.login,
        role: parse_role(&request.role)?,
        last_name: request.last_name,
        first_name: request.first_name,
        middle_name: request.middle_name,
        class_id: request
            .class_id
            .filter(|c| !c.is_empty())
            .map(|c| parse_uuid(&c, "class_id"))
            .transpose()?,
    };
    let user = service
        .create_user(&actor, new_user)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok((StatusCode::CREATED, Json(user.into())))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub class_id: Option<Option<String>>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub middle_name: Option<Option<String>>,
}

pub async fn update_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let user_id = parse_uuid(&user_id, "user id")?;
    let class_id = match request.class_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) if raw.is_empty() => Some(None),
        Some(Some(raw)) => Some(Some(parse_uuid(&raw, "class_id")?)),
    };
    let change = UserChange {
        role: request.role.map(|r| parse_role(&r)).transpose()?,
        class_id,
        is_active: request.is_active,
        last_name: request.last_name,
        first_name: request.first_name,
        middle_name: request.middle_name,
    };
    let user = service
        .update_user(&actor, user_id, change)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(user.into()))
}

#[derive(Debug, Deserialize)]
pub struct SetPasswordRequest {
    pub password: String,
}

/// `PUT /api/admin/users/:user_id/password`: sets the user's website password.
pub async fn set_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<SetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let user_id = parse_uuid(&user_id, "user id")?;
    service
        .set_password(&actor, user_id, &request.password)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Cabinets and lessons of a day
// ============================================================================

#[derive(Debug, Serialize)]
pub struct CabinetResponse {
    pub id: String,
    pub number: i32,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CabinetListResponse {
    pub cabinets: Vec<CabinetResponse>,
}

pub async fn cabinets_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CabinetListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let cabinets = service.cabinets(&actor).await.map_err(ApiError::from_admin_domain)?;
    Ok(Json(CabinetListResponse {
        cabinets: cabinets
            .into_iter()
            .map(|cabinet| CabinetResponse {
                id: cabinet.id.to_string(),
                number: cabinet.number,
                description: cabinet.description,
            })
            .collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateCabinetRequest {
    pub number: i32,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn create_cabinet_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCabinetRequest>,
) -> Result<(StatusCode, Json<CabinetResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let cabinet = service
        .create_cabinet(&actor, request.number, request.description)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok((
        StatusCode::CREATED,
        Json(CabinetResponse {
            id: cabinet.id.to_string(),
            number: cabinet.number,
            description: cabinet.description,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct PersonResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CabinetRef {
    pub id: String,
    pub number: i32,
}

#[derive(Debug, Serialize)]
pub struct AdminLessonResponse {
    pub id: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub subject_name: String,
    /// Class ("2027б") or group.
    pub target_name: String,
    /// The usual teachers of the course.
    pub teachers: Vec<PersonResponse>,
    pub substitute_teacher: Option<PersonResponse>,
    pub note: Option<String>,
    pub cabinet: Option<CabinetRef>,
    /// scheduled | completed | cancelled.
    pub status: String,
    pub has_homework: bool,
    /// Added by an admin to this date only (one-off or moved lesson).
    pub is_extra: bool,
}

impl From<ScheduleLesson> for AdminLessonResponse {
    fn from(lesson: ScheduleLesson) -> Self {
        Self {
            id: lesson.instance.id.to_string(),
            date: lesson.instance.lesson_date.to_string(),
            start_time: lesson.start_time.format("%H:%M").to_string(),
            end_time: lesson.end_time.format("%H:%M").to_string(),
            subject_name: lesson.subject.name,
            target_name: lesson.target_name,
            teachers: lesson
                .teachers
                .iter()
                .map(|teacher| PersonResponse {
                    id: teacher.id.to_string(),
                    name: full_name(teacher),
                })
                .collect(),
            substitute_teacher: lesson.substitute_teacher.as_ref().map(|teacher| PersonResponse {
                id: teacher.id.to_string(),
                name: full_name(teacher),
            }),
            note: lesson.substitution.and_then(|s| s.note),
            cabinet: lesson.cabinet.map(|cabinet| CabinetRef {
                id: cabinet.id.to_string(),
                number: cabinet.number,
            }),
            status: lesson.instance.status.to_string(),
            has_homework: lesson.homework.is_some(),
            is_extra: lesson.is_extra,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminDayResponse {
    pub date: String,
    pub lessons: Vec<AdminLessonResponse>,
}

#[derive(Debug, Deserialize)]
pub struct DateQuery {
    pub date: String,
}

pub async fn lessons_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DateQuery>,
) -> Result<Json<AdminDayResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let date = parse_date(&query.date)?;
    let day = service
        .lessons_for_date(&actor, date)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(AdminDayResponse {
        date: day.date.to_string(),
        lessons: day.lessons.into_iter().map(AdminLessonResponse::from).collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateLessonRequest {
    /// scheduled | cancelled | completed.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub cabinet_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub substitute_teacher_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub note: Option<Option<String>>,
}

fn optional_uuid(value: Option<Option<String>>, what: &str) -> Result<Option<Option<Uuid>>, ApiError> {
    match value {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(raw)) if raw.is_empty() => Ok(Some(None)),
        Some(Some(raw)) => Ok(Some(Some(parse_uuid(&raw, what)?))),
    }
}

pub async fn update_lesson_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
    Json(request): Json<UpdateLessonRequest>,
) -> Result<Json<AdminLessonResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let instance_id = parse_uuid(&instance_id, "lesson id")?;
    let change = LessonChange {
        status: request
            .status
            .map(|s| {
                LessonInstanceStatus::from_str(&s)
                    .map_err(|_| bad_request("Status must be scheduled, cancelled or completed"))
            })
            .transpose()?,
        cabinet_id: optional_uuid(request.cabinet_id, "cabinet_id")?,
        substitute_teacher_id: optional_uuid(request.substitute_teacher_id, "substitute_teacher_id")?,
        note: request.note,
    };
    let lesson = service
        .update_lesson(&actor, instance_id, change)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(lesson.into()))
}

// ============================================================================
// Schedule weeks
// ============================================================================

#[derive(Debug, Serialize)]
pub struct WeekResponse {
    pub week_start_date: String,
    /// draft | published.
    pub status: String,
    pub copied_from: Option<String>,
}

impl From<ScheduleWeek> for WeekResponse {
    fn from(week: ScheduleWeek) -> Self {
        Self {
            week_start_date: week.week_start_date.to_string(),
            status: if week.is_published() { "published" } else { "draft" }.to_string(),
            copied_from: week.copied_from.map(|d| d.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WeekListResponse {
    pub weeks: Vec<WeekResponse>,
}

pub async fn weeks_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<WeekListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let mut weeks = service.weeks(&actor).await.map_err(ApiError::from_admin_domain)?;
    weeks.sort_by_key(|week| std::cmp::Reverse(week.week_start_date));
    Ok(Json(WeekListResponse {
        weeks: weeks.into_iter().map(WeekResponse::from).collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateWeekRequest {
    /// Monday, `YYYY-MM-DD`.
    pub week_start_date: String,
    /// empty | templates | copy.
    pub source: String,
    /// Monday of the week to copy, for `source = copy`.
    #[serde(default)]
    pub copy_from: Option<String>,
}

pub async fn create_week_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWeekRequest>,
) -> Result<(StatusCode, Json<WeekResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let monday = parse_date(&request.week_start_date)?;
    let source = match request.source.as_str() {
        "empty" => WeekSource::Empty,
        "templates" => WeekSource::Templates,
        "copy" => WeekSource::CopyFrom(parse_date(
            request.copy_from.as_deref().ok_or_else(|| bad_request("copy_from is required"))?,
        )?),
        _ => return Err(bad_request("Source must be empty, templates or copy")),
    };
    let week = service
        .create_week(&actor, monday, source)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok((StatusCode::CREATED, Json(week.into())))
}

#[derive(Debug, Deserialize)]
pub struct UpdateWeekRequest {
    /// published | draft.
    pub status: String,
}

pub async fn update_week_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(week_start_date): Path<String>,
    Json(request): Json<UpdateWeekRequest>,
) -> Result<Json<WeekResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let monday = parse_date(&week_start_date)?;
    let published = match request.status.as_str() {
        "published" => true,
        "draft" => false,
        _ => return Err(bad_request("Status must be published or draft")),
    };
    let week = service
        .set_week_published(&actor, monday, published)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(week.into()))
}

// ============================================================================
// Announcements
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AnnouncementResponse {
    pub id: String,
    pub title: Option<String>,
    /// MarkdownV2 (Telegram).
    pub content: Option<String>,
    /// draft | published | archived.
    pub status: String,
    /// low | normal | high | urgent.
    pub priority: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

impl From<Announcement> for AnnouncementResponse {
    fn from(a: Announcement) -> Self {
        Self {
            id: a.id.to_string(),
            title: a.title,
            content: a.content,
            status: a.status.to_string(),
            priority: a.priority.to_string(),
            expires_at: a.expires_at.map(|t| t.to_rfc3339()),
            created_at: a.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminAnnouncementResponse {
    #[serde(flatten)]
    pub announcement: AnnouncementResponse,
    pub class_ids: Vec<String>,
    pub group_ids: Vec<String>,
    pub user_ids: Vec<String>,
    pub author_name: String,
}

impl From<AdminAnnouncementRow> for AdminAnnouncementResponse {
    fn from(row: AdminAnnouncementRow) -> Self {
        let ids = |ids: Vec<Uuid>| ids.into_iter().map(|id| id.to_string()).collect();
        Self {
            announcement: row.announcement.into(),
            class_ids: ids(row.class_ids),
            group_ids: ids(row.group_ids),
            user_ids: ids(row.user_ids),
            author_name: row.author_name,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminAnnouncementListResponse {
    pub announcements: Vec<AdminAnnouncementResponse>,
}

#[derive(Debug, Serialize)]
pub struct AnnouncementListResponse {
    pub announcements: Vec<AnnouncementResponse>,
}

#[derive(Debug, Deserialize)]
pub struct AnnouncementRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Empty class, group and user lists = everyone.
    #[serde(default)]
    pub class_ids: Vec<String>,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub user_ids: Vec<String>,
}

impl AnnouncementRequest {
    fn into_input(self) -> Result<AnnouncementInput, ApiError> {
        let ids = |raw: Vec<String>, what: &str| -> Result<Vec<Uuid>, ApiError> {
            raw.iter().map(|id| parse_uuid(id, what)).collect()
        };
        Ok(AnnouncementInput {
            title: self.title,
            content: self.content,
            status: match self.status.as_deref() {
                None | Some("") => AnnouncementStatus::Draft,
                Some(s) => AnnouncementStatus::from_str(s)
                    .map_err(|_| bad_request("Status must be draft, published or archived"))?,
            },
            priority: match self.priority.as_deref() {
                None | Some("") => AnnouncementPriority::Normal,
                Some(p) => AnnouncementPriority::from_str(p)
                    .map_err(|_| bad_request("Priority must be low, normal, high or urgent"))?,
            },
            expires_at: match self.expires_at.as_deref() {
                None | Some("") => None,
                Some(raw) => Some(
                    DateTime::parse_from_rfc3339(raw)
                        .map_err(|_| bad_request("expires_at must be RFC 3339"))?
                        .with_timezone(&Utc),
                ),
            },
            class_ids: ids(self.class_ids, "class id")?,
            group_ids: ids(self.group_ids, "group id")?,
            user_ids: ids(self.user_ids, "user id")?,
        })
    }
}

pub async fn list_announcements_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminAnnouncementListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let rows = service
        .list_announcements(&actor)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(AdminAnnouncementListResponse {
        announcements: rows.into_iter().map(AdminAnnouncementResponse::from).collect(),
    }))
}

pub async fn create_announcement_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AnnouncementRequest>,
) -> Result<(StatusCode, Json<AnnouncementResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let announcement = service
        .create_announcement(&actor, request.into_input()?)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok((StatusCode::CREATED, Json(announcement.into())))
}

pub async fn update_announcement_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(announcement_id): Path<String>,
    Json(request): Json<AnnouncementRequest>,
) -> Result<Json<AnnouncementResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let id = parse_uuid(&announcement_id, "announcement id")?;
    let announcement = service
        .update_announcement(&actor, id, request.into_input()?)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(announcement.into()))
}

pub async fn delete_announcement_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(announcement_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let id = parse_uuid(&announcement_id, "announcement id")?;
    service
        .delete_announcement(&actor, id)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/announcements`: what the current user (or the agent's user) should see.
pub async fn my_announcements_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AnnouncementListResponse>, ApiError> {
    let service = service(&state)?;
    let user = require_authenticated_user(&state, &headers).await?;
    let announcements = service
        .visible_announcements(&user)
        .await
        .map_err(ApiError::from_admin_domain)?;
    Ok(Json(AnnouncementListResponse {
        announcements: announcements.into_iter().map(AnnouncementResponse::from).collect(),
    }))
}
