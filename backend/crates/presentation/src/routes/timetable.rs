//! Admin timetable routes (`/api/admin/*`): subjects, groups, courses and their
//! teachers, the regular timetable of a class (slots), one-off and moved lessons.
//!
//! Access: same as `routes::admin` (a web session of an active admin, never the agent).
//!
//! Formats: days `mon`..`sat`, parity `every | odd | even`, times `HH:MM`,
//! dates `YYYY-MM-DD`. `apply_from` (optional date) also changes already created
//! weeks from that date on; without it only future generated weeks change.

use std::str::FromStr;

use application::use_cases::admin::{
    ClassTimetable, Course, Slot, SlotInput, TimetableChange, TimetableService,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{NaiveDate, NaiveTime};
use domain::{
    entities::user::User,
    errors::DomainError,
    value_objects::{day_of_week::DayOfWeek, lesson_target::LessonTarget, week_parity::WeekParity},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::router::{require_caller, ApiError, AppState};
use crate::routes::admin::{AdminLessonResponse, PersonResponse};

// ============================================================================
// Errors and parsing
// ============================================================================

fn error(error: DomainError) -> ApiError {
    let (status, message) = match error {
        DomainError::SubjectNotFound => (StatusCode::NOT_FOUND, "Subject not found"),
        DomainError::StudentGroupNotFound => (StatusCode::NOT_FOUND, "Group not found"),
        DomainError::LessonNotFound => (StatusCode::NOT_FOUND, "Course not found"),
        DomainError::LessonTemplateNotFound => (StatusCode::NOT_FOUND, "Slot not found"),
        DomainError::SubjectAlreadyExists => (StatusCode::CONFLICT, "Subject already exists"),
        DomainError::LessonAlreadyExists => (
            StatusCode::CONFLICT,
            "This class or group already has this subject",
        ),
        DomainError::LessonTemplateAlreadyExists => (
            StatusCode::CONFLICT,
            "This course already has a slot at this time",
        ),
        DomainError::LessonTemplateSlotConflict => (
            StatusCode::CONFLICT,
            "This course already has an overlapping slot",
        ),
        DomainError::LessonInstanceAlreadyExists => {
            (StatusCode::CONFLICT, "This lesson is already on that date")
        }
        DomainError::InvalidSubjectNameFormat => {
            (StatusCode::UNPROCESSABLE_ENTITY, "Subject name is required")
        }
        DomainError::InvalidLessonTemplateTime => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "End time must be after start time",
        ),
        DomainError::InvalidLessonInstanceDate => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "There are no lessons on Sunday",
        ),
        DomainError::UserNotFound => (StatusCode::NOT_FOUND, "Teacher not found"),
        DomainError::UserIsInactive => (StatusCode::CONFLICT, "Teacher account is inactive"),
        other => return ApiError::from_admin_domain(other),
    };
    ApiError {
        status,
        message: message.to_string(),
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

fn parse_optional_uuid(raw: Option<&str>, what: &str) -> Result<Option<Uuid>, ApiError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => parse_uuid(raw, what).map(Some),
    }
}

fn parse_date(raw: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| bad_request("Invalid date, expected YYYY-MM-DD"))
}

fn parse_optional_date(raw: Option<&str>) -> Result<Option<NaiveDate>, ApiError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => parse_date(raw).map(Some),
    }
}

fn parse_time(raw: &str) -> Result<NaiveTime, ApiError> {
    let raw = raw.trim();
    NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(raw, "%H:%M:%S"))
        .map_err(|_| bad_request("Invalid time, expected HH:MM"))
}

fn parse_uuids(raw: &[String], what: &str) -> Result<Vec<Uuid>, ApiError> {
    raw.iter().map(|id| parse_uuid(id, what)).collect()
}

fn service(state: &AppState) -> Result<&TimetableService, ApiError> {
    state.timetable_service.as_deref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Timetable service not initialized".to_string(),
    })
}

/// The admin making the request; the agent and non-admins get 403.
async fn admin<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
) -> Result<(User, &'a TimetableService), ApiError> {
    let service = service(state)?;
    let caller = require_caller(state, headers).await?;
    if caller.is_agent || !caller.user.is_admin() {
        return Err(error(DomainError::InsufficientPermissions));
    }
    Ok((caller.user, service))
}

fn person(user: &User) -> PersonResponse {
    PersonResponse {
        id: user.id.to_string(),
        name: format!("{} {}", user.last_name, user.first_name),
    }
}

// ============================================================================
// Subjects and groups
// ============================================================================

#[derive(Debug, Serialize)]
pub struct NamedResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct SubjectListResponse {
    pub subjects: Vec<NamedResponse>,
}

pub async fn subjects_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SubjectListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let subjects = service.subjects(&actor).await.map_err(error)?;
    Ok(Json(SubjectListResponse {
        subjects: subjects
            .into_iter()
            .map(|subject| NamedResponse {
                id: subject.id.to_string(),
                name: subject.name,
            })
            .collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateSubjectRequest {
    pub name: String,
}

pub async fn create_subject_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSubjectRequest>,
) -> Result<(StatusCode, Json<NamedResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let subject = service
        .create_subject(&actor, &request.name)
        .await
        .map_err(error)?;
    Ok((
        StatusCode::CREATED,
        Json(NamedResponse {
            id: subject.id.to_string(),
            name: subject.name,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct GroupListResponse {
    pub groups: Vec<NamedResponse>,
}

pub async fn groups_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<GroupListResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let groups = service.groups(&actor).await.map_err(error)?;
    Ok(Json(GroupListResponse {
        groups: groups
            .into_iter()
            .map(|group| NamedResponse {
                id: group.id.to_string(),
                name: group.name,
            })
            .collect(),
    }))
}

// ============================================================================
// Courses
// ============================================================================

#[derive(Debug, Serialize)]
pub struct CourseResponse {
    pub id: String,
    pub subject: NamedResponse,
    /// Set for a class course.
    pub class_id: Option<String>,
    /// Set for a group course.
    pub group_id: Option<String>,
    /// "2027б" or the group name.
    pub target_name: String,
    pub teachers: Vec<PersonResponse>,
}

impl From<Course> for CourseResponse {
    fn from(course: Course) -> Self {
        Self {
            id: course.lesson.id.to_string(),
            subject: NamedResponse {
                id: course.subject.id.to_string(),
                name: course.subject.name,
            },
            class_id: course.lesson.class_id().map(|id| id.to_string()),
            group_id: course.lesson.group_id().map(|id| id.to_string()),
            target_name: course.target_name,
            teachers: course.teachers.iter().map(person).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCourseRequest {
    /// Exactly one of `class_id` and `group_id`.
    #[serde(default)]
    pub class_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    pub subject_id: String,
    #[serde(default)]
    pub teacher_ids: Vec<String>,
}

pub async fn create_course_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCourseRequest>,
) -> Result<(StatusCode, Json<CourseResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let class_id = parse_optional_uuid(request.class_id.as_deref(), "class_id")?;
    let group_id = parse_optional_uuid(request.group_id.as_deref(), "group_id")?;
    let target = match (class_id, group_id) {
        (Some(class_id), None) => LessonTarget::Class(class_id),
        (None, Some(group_id)) => LessonTarget::Group(group_id),
        _ => return Err(bad_request("Give exactly one of class_id and group_id")),
    };
    let subject_id = parse_uuid(&request.subject_id, "subject_id")?;
    let teacher_ids = parse_uuids(&request.teacher_ids, "teacher id")?;
    let course = service
        .create_course(&actor, target, subject_id, &teacher_ids)
        .await
        .map_err(error)?;
    Ok((StatusCode::CREATED, Json(course.into())))
}

#[derive(Debug, Deserialize)]
pub struct CourseTeachersRequest {
    pub teacher_ids: Vec<String>,
}

pub async fn set_course_teachers_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(lesson_id): Path<String>,
    Json(request): Json<CourseTeachersRequest>,
) -> Result<Json<CourseResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let lesson_id = parse_uuid(&lesson_id, "course id")?;
    let teacher_ids = parse_uuids(&request.teacher_ids, "teacher id")?;
    let course = service
        .set_course_teachers(&actor, lesson_id, &teacher_ids)
        .await
        .map_err(error)?;
    Ok(Json(course.into()))
}

// ============================================================================
// Regular timetable
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SlotResponse {
    pub id: String,
    pub course_id: String,
    pub subject_name: String,
    pub target_name: String,
    pub teachers: Vec<PersonResponse>,
    /// mon..sat
    pub day: String,
    pub start_time: String,
    pub end_time: String,
    /// every | odd | even
    pub parity: String,
    pub cabinet_id: Option<String>,
    pub cabinet_number: Option<i32>,
}

impl From<Slot> for SlotResponse {
    fn from(slot: Slot) -> Self {
        Self {
            id: slot.template.id.to_string(),
            course_id: slot.course.lesson.id.to_string(),
            subject_name: slot.course.subject.name,
            target_name: slot.course.target_name,
            teachers: slot.course.teachers.iter().map(person).collect(),
            day: slot.template.day.to_string(),
            start_time: slot.template.start_time.format("%H:%M").to_string(),
            end_time: slot.template.end_time.format("%H:%M").to_string(),
            parity: slot.template.parity.to_string(),
            cabinet_id: slot.template.cabinet_id.map(|id| id.to_string()),
            cabinet_number: slot.cabinet_number,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ClassTimetableResponse {
    pub courses: Vec<CourseResponse>,
    pub slots: Vec<SlotResponse>,
}

impl From<ClassTimetable> for ClassTimetableResponse {
    fn from(timetable: ClassTimetable) -> Self {
        Self {
            courses: timetable
                .courses
                .into_iter()
                .map(CourseResponse::from)
                .collect(),
            slots: timetable
                .slots
                .into_iter()
                .map(SlotResponse::from)
                .collect(),
        }
    }
}

pub async fn class_timetable_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(class_id): Path<String>,
) -> Result<Json<ClassTimetableResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let class_id = parse_uuid(&class_id, "class id")?;
    let timetable = service
        .class_timetable(&actor, class_id)
        .await
        .map_err(error)?;
    Ok(Json(timetable.into()))
}

#[derive(Debug, Serialize)]
pub struct TimetableChangeResponse {
    /// Lessons added, moved or removed in existing weeks.
    pub changed: usize,
    /// Lessons left in place because they have homework.
    pub kept: usize,
}

impl From<TimetableChange> for TimetableChangeResponse {
    fn from(change: TimetableChange) -> Self {
        Self {
            changed: change.changed,
            kept: change.kept,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SlotRequest {
    pub course_id: String,
    pub day: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub parity: Option<String>,
    #[serde(default)]
    pub cabinet_id: Option<String>,
    #[serde(default)]
    pub apply_from: Option<String>,
}

impl SlotRequest {
    fn parse(&self) -> Result<(SlotInput, Option<NaiveDate>), ApiError> {
        let input = SlotInput {
            lesson_id: parse_uuid(&self.course_id, "course_id")?,
            day: DayOfWeek::from_str(self.day.trim())
                .map_err(|_| bad_request("Day must be mon, tue, wed, thu, fri or sat"))?,
            start_time: parse_time(&self.start_time)?,
            end_time: parse_time(&self.end_time)?,
            parity: match self.parity.as_deref().map(str::trim) {
                None | Some("") => WeekParity::Every,
                Some(raw) => WeekParity::from_str(raw)
                    .map_err(|_| bad_request("Parity must be every, odd or even"))?,
            },
            cabinet_id: parse_optional_uuid(self.cabinet_id.as_deref(), "cabinet_id")?,
        };
        Ok((input, parse_optional_date(self.apply_from.as_deref())?))
    }
}

pub async fn add_slot_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SlotRequest>,
) -> Result<(StatusCode, Json<TimetableChangeResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let (input, apply_from) = request.parse()?;
    let change = service
        .add_slot(&actor, input, apply_from)
        .await
        .map_err(error)?;
    Ok((StatusCode::CREATED, Json(change.into())))
}

pub async fn update_slot_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(template_id): Path<String>,
    Json(request): Json<SlotRequest>,
) -> Result<Json<TimetableChangeResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let template_id = parse_uuid(&template_id, "slot id")?;
    let (input, apply_from) = request.parse()?;
    let change = service
        .update_slot(&actor, template_id, input, apply_from)
        .await
        .map_err(error)?;
    Ok(Json(change.into()))
}

#[derive(Debug, Deserialize)]
pub struct ApplyFromQuery {
    #[serde(default)]
    pub apply_from: Option<String>,
}

pub async fn remove_slot_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(template_id): Path<String>,
    Query(query): Query<ApplyFromQuery>,
) -> Result<Json<TimetableChangeResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let template_id = parse_uuid(&template_id, "slot id")?;
    let apply_from = parse_optional_date(query.apply_from.as_deref())?;
    let change = service
        .remove_slot(&actor, template_id, apply_from)
        .await
        .map_err(error)?;
    Ok(Json(change.into()))
}

// ============================================================================
// One-off lessons
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct OneOffLessonRequest {
    pub course_id: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub cabinet_id: Option<String>,
}

pub async fn add_one_off_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OneOffLessonRequest>,
) -> Result<(StatusCode, Json<AdminLessonResponse>), ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let lesson = service
        .add_one_off(
            &actor,
            parse_uuid(&request.course_id, "course_id")?,
            parse_date(&request.date)?,
            parse_time(&request.start_time)?,
            parse_time(&request.end_time)?,
            parse_optional_uuid(request.cabinet_id.as_deref(), "cabinet_id")?,
        )
        .await
        .map_err(error)?;
    Ok((StatusCode::CREATED, Json(lesson.into())))
}

#[derive(Debug, Deserialize)]
pub struct MoveLessonRequest {
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub cabinet_id: Option<String>,
}

/// Answers with the lesson at its new place.
pub async fn move_lesson_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
    Json(request): Json<MoveLessonRequest>,
) -> Result<Json<AdminLessonResponse>, ApiError> {
    let (actor, service) = admin(&state, &headers).await?;
    let lesson = service
        .move_lesson(
            &actor,
            parse_uuid(&instance_id, "lesson id")?,
            parse_date(&request.date)?,
            parse_time(&request.start_time)?,
            parse_time(&request.end_time)?,
            parse_optional_uuid(request.cabinet_id.as_deref(), "cabinet_id")?,
        )
        .await
        .map_err(error)?;
    Ok(Json(lesson.into()))
}
