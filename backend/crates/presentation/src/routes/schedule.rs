use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::NaiveDate;
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::use_cases::schedule::schedule::{
    HomeworkDetails, NumberedLesson, ScheduleDay, ScheduleLesson, WeeklySchedule,
};

use crate::router::{require_authenticated_user, ApiError, AppState};
use crate::routes::files::HomeworkFileResponse;

#[derive(Debug, Deserialize)]
pub struct ScheduleWeekQuery {
    pub date: String,
    #[serde(default)]
    pub student_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleWeekResponse {
    pub week_start_date: String,
    pub days: Vec<ScheduleDayResponse>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDayResponse {
    pub date: String,
    pub lessons: Vec<ScheduleLessonResponse>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleLessonResponse {
    pub id: String,
    pub start_time: String,
    pub end_time: String,
    pub subject_name: String,
    /// Class ("2027б") or group the lesson is for.
    pub target_name: String,
    /// Who gives the lesson: the substitute teacher if there is one.
    pub teachers: Vec<String>,
    /// The usual teachers when the lesson has a substitute, otherwise absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usual_teachers: Option<Vec<String>>,
    pub cabinet: Option<String>,
    /// scheduled | completed | cancelled: cancelled lessons stay in the list, shown greyed.
    pub status: String,
    /// Admin's note for this lesson ("в актовом зале").
    pub note: Option<String>,
    /// Added by an admin to this date only (one-off or moved lesson).
    pub is_extra: bool,
    pub homework: Option<HomeworkResponse>,
}

fn lesson_response(lesson: ScheduleLesson) -> ScheduleLessonResponse {
    let name = |teacher: &domain::entities::user::User| {
        format!("{} {}", teacher.last_name, teacher.first_name)
    };
    let usual_teachers: Vec<String> = lesson.teachers.iter().map(name).collect();
    let (teachers, usual_teachers) = match &lesson.substitute_teacher {
        Some(substitute) => (vec![name(substitute)], Some(usual_teachers)),
        None => (usual_teachers, None),
    };
    ScheduleLessonResponse {
        id: lesson.instance.id.to_string(),
        start_time: lesson.start_time.format("%H:%M").to_string(),
        end_time: lesson.end_time.format("%H:%M").to_string(),
        subject_name: lesson.subject.name,
        target_name: lesson.target_name,
        teachers,
        usual_teachers,
        cabinet: lesson.cabinet.map(|cabinet| cabinet.number.to_string()),
        status: lesson.instance.status.to_string(),
        note: lesson.substitution.and_then(|substitution| substitution.note),
        is_extra: lesson.is_extra,
        homework: lesson.homework.map(homework_response),
    }
}

fn week_response(week: WeeklySchedule) -> ScheduleWeekResponse {
    ScheduleWeekResponse {
        week_start_date: week.week_start_date.to_string(),
        days: week
            .days
            .into_iter()
            .map(|day| ScheduleDayResponse {
                date: day.date.to_string(),
                lessons: day.lessons.into_iter().map(lesson_response).collect(),
            })
            .collect(),
    }
}

fn day_response(day: ScheduleDay) -> ScheduleDayResponseDto {
    ScheduleDayResponseDto {
        date: day.date.to_string(),
        lessons: day.lessons.into_iter().map(lesson_response).collect(),
    }
}

#[derive(Debug, Serialize)]
pub struct HomeworkResponse {
    pub id: String,
    pub text: String,
    /// Attached files; contents via `GET /api/homework/{id}/files/{file_id}`.
    pub files: Vec<HomeworkFileResponse>,
}

fn homework_response(details: HomeworkDetails) -> HomeworkResponse {
    HomeworkResponse {
        id: details.homework.id.to_string(),
        text: details.homework.text_content.unwrap_or_default(),
        files: details.files.into_iter().map(HomeworkFileResponse::from).collect(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ClassScheduleWeekQuery {
    pub date: String,
    #[serde(default)]
    pub class_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleDayQuery {
    pub date: String,
    #[serde(default)]
    pub student_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDayResponseDto {
    pub date: String,
    pub lessons: Vec<ScheduleLessonResponse>,
}

#[derive(Debug, Deserialize)]
pub struct TeacherScheduleQuery {
    pub date: String,
    #[serde(default)]
    pub teacher_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClassScheduleDayQuery {
    pub date: String,
    #[serde(default)]
    pub class_id: Option<String>,
}

pub async fn schedule_week_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleWeekQuery>,
) -> Result<Json<ScheduleWeekResponse>, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| ApiError::from_schedule_domain(DomainError::InvalidLessonInstanceDate))?;

    let student_id = query
        .student_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::from_schedule_domain(DomainError::UserNotFound))?;


    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .current_week_schedule(&user, student_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    Ok(Json(week_response(response)))
}

pub async fn class_schedule_week_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ClassScheduleWeekQuery>,
) -> Result<Json<ScheduleWeekResponse>, ApiError> {
    let _user = require_authenticated_user(&state, &headers).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| ApiError::from_schedule_domain(DomainError::InvalidLessonInstanceDate))?;

    let class_id = query
        .class_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::from_schedule_domain(DomainError::ClassNotFound))?;

    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .class_week_schedule(class_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    Ok(Json(week_response(response)))
}

pub async fn day_schedule_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleDayQuery>,
) -> Result<Json<ScheduleDayResponseDto>, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| ApiError::from_schedule_domain(DomainError::InvalidLessonInstanceDate))?;

    let student_id = query
        .student_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::from_schedule_domain(DomainError::UserNotFound))?;


    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .day_schedule(&user, student_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    Ok(Json(day_response(response)))
}

pub async fn class_day_schedule_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ClassScheduleDayQuery>,
) -> Result<Json<ScheduleDayResponseDto>, ApiError> {
    let _user = require_authenticated_user(&state, &headers).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| ApiError::from_schedule_domain(DomainError::InvalidLessonInstanceDate))?;

    let class_id = query
        .class_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::from_schedule_domain(DomainError::ClassNotFound))?;

    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .class_day_schedule(class_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    Ok(Json(day_response(response)))
}

/// Teacher-schedule errors: an unknown id or a student id means "no such teacher".
fn teacher_schedule_error(error: DomainError) -> ApiError {
    match error {
        DomainError::UserNotFound => ApiError {
            status: StatusCode::NOT_FOUND,
            message: "Teacher not found".to_string(),
        },
        other => ApiError::from_schedule_domain(other),
    }
}

/// Validates `?date=YYYY-MM-DD&teacher_id=<uuid>` for the teacher routes.
async fn teacher_schedule_input(
    state: &AppState,
    headers: &HeaderMap,
    query: &TeacherScheduleQuery,
) -> Result<(NaiveDate, Uuid), ApiError> {
    // A school timetable is not secret: any logged-in user may look up a teacher.
    require_authenticated_user(state, headers).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| ApiError::from_schedule_domain(DomainError::InvalidLessonInstanceDate))?;
    let teacher_id = query
        .teacher_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| teacher_schedule_error(DomainError::UserNotFound))?;
    Ok((date, teacher_id))
}

/// `GET /api/teachers/schedule/week?date=YYYY-MM-DD&teacher_id=<uuid>` — the week
/// containing `date`: lessons of all courses the teacher is assigned to.
pub async fn teacher_schedule_week_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TeacherScheduleQuery>,
) -> Result<Json<ScheduleWeekResponse>, ApiError> {
    let (date, teacher_id) = teacher_schedule_input(&state, &headers, &query).await?;
    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let week = schedule_service
        .teacher_week_schedule(teacher_id, date)
        .await
        .map_err(teacher_schedule_error)?;

    Ok(Json(week_response(week)))
}

/// `GET /api/teachers/schedule/day?date=YYYY-MM-DD&teacher_id=<uuid>` — one day.
pub async fn teacher_schedule_day_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TeacherScheduleQuery>,
) -> Result<Json<ScheduleDayResponseDto>, ApiError> {
    let (date, teacher_id) = teacher_schedule_input(&state, &headers, &query).await?;
    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let day = schedule_service
        .teacher_day_schedule(teacher_id, date)
        .await
        .map_err(teacher_schedule_error)?;

    Ok(Json(day_response(day)))
}

#[derive(Debug, Serialize)]
pub struct NextLessonResponse {
    /// Number of the lesson in that day of the class/group ("Урок 3").
    pub number: usize,
    #[serde(flatten)]
    pub lesson: ScheduleLessonResponse,
}

#[derive(Debug, Serialize)]
pub struct NextLessonsResponse {
    /// `YYYY-MM-DD` of the next meeting.
    pub date: String,
    /// Its lessons in time order: two for a double lesson.
    pub lessons: Vec<NextLessonResponse>,
}

/// `GET /api/lessons/:lesson_instance_id/next` — the next published meeting of the same
/// course after this lesson (and after the other half of its double); 404 if there is none
/// yet, e.g. next week is not published.
pub async fn next_lessons_handler(
    State(state): State<AppState>,
    Path(lesson_instance_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<NextLessonsResponse>, ApiError> {
    require_authenticated_user(&state, &headers).await?;
    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;
    let instance_id = Uuid::parse_str(&lesson_instance_id).map_err(|_| ApiError {
        status: StatusCode::NOT_FOUND,
        message: "Lesson not found".to_string(),
    })?;
    let lessons = schedule_service
        .next_course_lessons(instance_id)
        .await
        .map_err(|error| match error {
            DomainError::LessonInstanceNotFound => ApiError {
                status: StatusCode::NOT_FOUND,
                message: "Lesson not found".to_string(),
            },
            other => ApiError::from_schedule_domain(other),
        })?;
    let Some(first) = lessons.first() else {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "Следующий урок ещё не в опубликованном расписании".to_string(),
        });
    };
    let date = first.lesson.instance.lesson_date.to_string();
    Ok(Json(NextLessonsResponse {
        date,
        lessons: lessons
            .into_iter()
            .map(|NumberedLesson { lesson, number }| NextLessonResponse {
                number,
                lesson: lesson_response(lesson),
            })
            .collect(),
    }))
}
