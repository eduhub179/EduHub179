use application::use_cases::schedule::ScheduleService;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::router::{ApiError, AppState};

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

pub fn schedule_router() -> Router {
    Router::new().route("/api/schedule/week", get(schedule_week_handler))
}

async fn schedule_week_handler(
    State(state): State<AppState>,
    Query(query): Query<ScheduleWeekQuery>,
) -> Result<Json<ScheduleWeekResponse>, ApiError> {
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
