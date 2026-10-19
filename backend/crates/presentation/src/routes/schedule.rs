use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::NaiveDate;
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};

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
    pub teachers: Vec<String>,
    pub cabinet: Option<String>,
    pub homework: Option<HomeworkResponse>,
}

#[derive(Debug, Serialize)]
pub struct HomeworkResponse {
    pub id: String,
    pub text: String,
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

    if user.role.is_student() && user.id != student_id {
        return Err(ApiError::from_schedule_domain(DomainError::InsufficientPermissions));
    }

    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .current_week_schedule(student_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    let days = response
        .days
        .into_iter()
        .map(|day| ScheduleDayResponse {
            date: day.date.to_string(),
            lessons: day
                .lessons
                .into_iter()
                .map(|lesson| ScheduleLessonResponse {
                    id: lesson.instance.id.to_string(),
                    start_time: lesson.start_time.format("%H:%M").to_string(),
                    end_time: lesson.end_time.format("%H:%M").to_string(),
                    subject_name: lesson.subject.name,
                    teachers: lesson
                        .teachers
                        .into_iter()
                        .map(|teacher| format!("{} {}", teacher.last_name, teacher.first_name))
                        .collect(),
                    cabinet: lesson.cabinet.map(|cabinet| cabinet.number.to_string()),
                    homework: lesson.homework.map(|homework| HomeworkResponse {
                        id: homework.homework.id.to_string(),
                        text: homework
                            .homework
                            .text_content
                            .unwrap_or_else(|| String::new()),
                    }),
                })
                .collect(),
        })
        .collect();

    Ok(Json(ScheduleWeekResponse {
        week_start_date: response.week_start_date.to_string(),
        days,
    }))
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

    let days = response
        .days
        .into_iter()
        .map(|day| ScheduleDayResponse {
            date: day.date.to_string(),
            lessons: day
                .lessons
                .into_iter()
                .map(|lesson| ScheduleLessonResponse {
                    id: lesson.instance.id.to_string(),
                    start_time: lesson.start_time.format("%H:%M").to_string(),
                    end_time: lesson.end_time.format("%H:%M").to_string(),
                    subject_name: lesson.subject.name,
                    teachers: lesson
                        .teachers
                        .into_iter()
                        .map(|teacher| format!("{} {}", teacher.last_name, teacher.first_name))
                        .collect(),
                    cabinet: lesson.cabinet.map(|cabinet| cabinet.number.to_string()),
                    homework: lesson.homework.map(|homework| HomeworkResponse {
                        id: homework.homework.id.to_string(),
                        text: homework
                            .homework
                            .text_content
                            .unwrap_or_else(|| String::new()),
                    }),
                })
                .collect(),
        })
        .collect();

    Ok(Json(ScheduleWeekResponse {
        week_start_date: response.week_start_date.to_string(),
        days,
    }))
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

    if user.role.is_student() && user.id != student_id {
        return Err(ApiError::from_schedule_domain(DomainError::InsufficientPermissions));
    }

    let schedule_service = state.schedule_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Schedule service not initialized".to_string(),
    })?;

    let response = schedule_service
        .day_schedule(student_id, date)
        .await
        .map_err(ApiError::from_schedule_domain)?;

    let lessons = response
        .lessons
        .into_iter()
        .map(|lesson| ScheduleLessonResponse {
            id: lesson.instance.id.to_string(),
            start_time: lesson.start_time.format("%H:%M").to_string(),
            end_time: lesson.end_time.format("%H:%M").to_string(),
            subject_name: lesson.subject.name,
            teachers: lesson
                .teachers
                .into_iter()
                .map(|teacher| format!("{} {}", teacher.last_name, teacher.first_name))
                .collect(),
            cabinet: lesson.cabinet.map(|cabinet| cabinet.number.to_string()),
            homework: lesson.homework.map(|homework| HomeworkResponse {
                id: homework.homework.id.to_string(),
                text: homework
                    .homework
                    .text_content
                    .unwrap_or_else(|| String::new()),
            }),
        })
        .collect();

    Ok(Json(ScheduleDayResponseDto {
        date: response.date.to_string(),
        lessons,
    }))
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

    let lessons = response
        .lessons
        .into_iter()
        .map(|lesson| ScheduleLessonResponse {
            id: lesson.instance.id.to_string(),
            start_time: lesson.start_time.format("%H:%M").to_string(),
            end_time: lesson.end_time.format("%H:%M").to_string(),
            subject_name: lesson.subject.name,
            teachers: lesson
                .teachers
                .into_iter()
                .map(|teacher| format!("{} {}", teacher.last_name, teacher.first_name))
                .collect(),
            cabinet: lesson.cabinet.map(|cabinet| cabinet.number.to_string()),
            homework: lesson.homework.map(|homework| HomeworkResponse {
                id: homework.homework.id.to_string(),
                text: homework
                    .homework
                    .text_content
                    .unwrap_or_else(|| String::new()),
            }),
        })
        .collect();

    Ok(Json(ScheduleDayResponseDto {
        date: response.date.to_string(),
        lessons,
    }))
}
