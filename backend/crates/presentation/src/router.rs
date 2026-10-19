//! Axum router for the HTTP API.
//!
//! This is the initial MVP. It exposes health, auth routes, and schedule routes.

use application::use_cases::{
    auth::{
        login::{LoginCommand, LoginUseCase},
        OneTimeCodeService,
    },
    homework::{
        create_homework::{CreateHomeworkCommand, CreateHomeworkUseCase},
        update_homework_text::{UpdateHomeworkTextCommand, UpdateHomeworkTextUseCase},
    },
    schedule::ScheduleService,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::NaiveDate;
use domain::{
    errors::DomainError,
    repositories::{
        class_repository::ClassRepository,
        telegram_bindigs_repository::TelegramBindingRepository,
        user_repository::UserRepository,
    },
    value_objects::{homework_status::HomeworkStatus, role::UserRole},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc};
use tower_http::cors::{CorsLayer};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: PublicUser,
}

#[derive(Debug, Serialize)]
pub struct PublicUser {
    pub id: String,
    pub login: String,
    pub role: String,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: Option<String>,
    pub is_active: bool,
    pub class_id: Option<String>,
}

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

#[derive(Debug, Serialize)]
pub struct ClassListItemResponse {
    pub id: String,
    pub graduation_year: i32,
    pub letter: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct ClassListResponse {
    pub classes: Vec<ClassListItemResponse>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn from_auth_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Invalid login or password")
            }
            DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "User account is inactive"),
            DomainError::InvalidLoginFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid login format")
            }
            DomainError::InvalidEmailFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid email format")
            }
            DomainError::InvalidAuthCode => {
                (StatusCode::UNAUTHORIZED, "Invalid or expired auth code")
            }
            DomainError::UserNotFound => (StatusCode::UNAUTHORIZED, "Invalid login or password"),
            DomainError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }

    fn from_schedule_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::UserNotFound => (StatusCode::NOT_FOUND, "Student not found"),
            DomainError::UserIsInactive => (StatusCode::FORBIDDEN, "User account is inactive"),
            DomainError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            DomainError::ScheduleWeekNotFound => {
                (StatusCode::NOT_FOUND, "Schedule week not found")
            }
            DomainError::InvalidLessonInstanceDate => {
                (StatusCode::BAD_REQUEST, "Invalid lesson date")
            }
            DomainError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }

    fn from_homework_domain(error: DomainError) -> Self {
        let (status, message) = match error {
            DomainError::InvalidHomeworkTextFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Invalid homework text format")
            }
            DomainError::HomeworkAlreadyExists => {
                (StatusCode::CONFLICT, "Homework already exists for this lesson")
            }
            DomainError::LessonInstanceNotFound => {
                (StatusCode::NOT_FOUND, "Lesson instance not found")
            }
            DomainError::HomeworkNotFound => (StatusCode::NOT_FOUND, "Homework not found"),
            DomainError::InvalidHomeworkStatus => {
                (StatusCode::BAD_REQUEST, "Invalid homework status")
            }
            DomainError::InvalidNameFormat => (StatusCode::BAD_REQUEST, "Invalid role format"),
            DomainError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            DomainError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
            _ => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        Self {
            status,
            message: message.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(json!({
            "error": self.message,
        }));

        (self.status, body).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub login_use_case: Option<Arc<LoginUseCase>>,
    pub schedule_service: Option<Arc<ScheduleService>>,
    pub create_homework_use_case: Option<Arc<CreateHomeworkUseCase>>,
    pub update_homework_use_case: Option<Arc<UpdateHomeworkTextUseCase>>,
    pub class_repository: Option<Arc<dyn ClassRepository>>,
    pub user_repository: Option<Arc<dyn UserRepository>>,
    pub one_time_code_service: Option<Arc<OneTimeCodeService>>,
    pub telegram_lookup: Option<TelegramLookup>,
}

/// What `GET /api/telegram/users/{telegram_id}` needs: active bindings, users, and the agent key.
#[derive(Clone)]
pub struct TelegramLookup {
    pub bindings: Arc<dyn TelegramBindingRepository>,
    pub users: Arc<dyn UserRepository>,
    /// Shared secret of the chat agent (`X-Agent-Key` header). Route is closed when empty.
    pub agent_key: String,
}

fn cors_layer() -> CorsLayer {
    let allow_origin = [
        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
        "http://localhost:5174".parse::<HeaderValue>().unwrap(),
        "http://127.0.0.1:5173".parse::<HeaderValue>().unwrap(),
        "http://127.0.0.1:5174".parse::<HeaderValue>().unwrap(),
    ];

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::PATCH])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderName::from_static("x-internal-secret"),
        ])
        .allow_credentials(true)
}

pub fn create_app() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .layer(cors_layer())
}

pub fn create_app_with_auth(login_use_case: Arc<LoginUseCase>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/code/send", post(send_auth_code_handler))
        .route("/api/auth/code/verify", post(verify_auth_code_handler))
        .route("/api/schedule/week", get(schedule_week_handler))
        .route("/api/schedule/day", get(day_schedule_handler))
        .route("/api/classes/schedule/week", get(class_schedule_week_handler))
        .route("/api/classes/schedule/day", get(class_day_schedule_handler))
        .route("/api/homework", post(create_homework_handler))
        .route("/api/homework/:homework_id/text", patch(update_homework_text_handler))
        .route("/api/classes", get(classes_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: Some(login_use_case),
            schedule_service: None,
            create_homework_use_case: None,
            update_homework_use_case: None,
            class_repository: None,
            user_repository: None,
            one_time_code_service: None,
            telegram_lookup: None,
        })
}

pub fn create_app_with_schedule(schedule_service: Arc<ScheduleService>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/schedule/week", get(schedule_week_handler))
        .route("/api/schedule/day", get(day_schedule_handler))
        .route("/api/classes/schedule/week", get(class_schedule_week_handler))
        .route("/api/classes/schedule/day", get(class_day_schedule_handler))
        .route("/api/homework", post(create_homework_handler))
        .route("/api/homework/:homework_id/text", patch(update_homework_text_handler))
        .route("/api/classes", get(classes_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: None,
            schedule_service: Some(schedule_service),
            create_homework_use_case: None,
            update_homework_use_case: None,
            class_repository: None,
            user_repository: None,
            one_time_code_service: None,
            telegram_lookup: None,
        })
}

pub fn create_app_with_auth_and_schedule(
    login_use_case: Arc<LoginUseCase>,
    schedule_service: Arc<ScheduleService>,
    create_homework_use_case: Arc<CreateHomeworkUseCase>,
    update_homework_use_case: Arc<UpdateHomeworkTextUseCase>,
    class_repository: Arc<dyn ClassRepository>,
    user_repository: Arc<dyn UserRepository>,
    one_time_code_service: Arc<OneTimeCodeService>,
    telegram_lookup: Option<TelegramLookup>,
) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/code/send", post(send_auth_code_handler))
        .route("/api/auth/code/verify", post(verify_auth_code_handler))
        .route("/api/schedule/week", get(schedule_week_handler))
        .route("/api/schedule/day", get(day_schedule_handler))
        .route("/api/classes/schedule/week", get(class_schedule_week_handler))
        .route("/api/classes/schedule/day", get(class_day_schedule_handler))
        .route("/api/homework", post(create_homework_handler))
        .route("/api/homework/:homework_id/text", patch(update_homework_text_handler))
        .route("/api/classes", get(classes_handler))
        .route("/api/telegram/users/:telegram_id", get(telegram_user_handler))
        .layer(cors_layer())
        .with_state(AppState {
            login_use_case: Some(login_use_case),
            schedule_service: Some(schedule_service),
            create_homework_use_case: Some(create_homework_use_case),
            update_homework_use_case: Some(update_homework_use_case),
            class_repository: Some(class_repository),
            user_repository: Some(user_repository),
            one_time_code_service: Some(one_time_code_service),
            telegram_lookup,
        })
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "eduhub-api"
    }))
}

async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let login_use_case = state.login_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Login use case not initialized".to_string(),
    })?;

    let command = LoginCommand::try_new(payload.login, payload.password)
        .map_err(ApiError::from_auth_domain)?;

    let session = login_use_case
        .execute(command)
        .await
        .map_err(ApiError::from_auth_domain)?;

    Ok(Json(LoginResponse {
        token: session.token,
        user: PublicUser {
            id: session.user.id.to_string(),
            login: session.user.login,
            role: session.user.role.to_string(),
            first_name: session.user.first_name,
            last_name: session.user.last_name,
            middle_name: session.user.middle_name,
            is_active: session.user.is_active,
            class_id: session.user.class_id.map(|id| id.to_string()),
        },
    }))
}

async fn create_homework_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateHomeworkRequest>,
) -> Result<Json<CreateHomeworkResponse>, ApiError> {
    let create_homework_use_case = state.create_homework_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Create homework use case not initialized".to_string(),
    })?;

    let lesson_instance_id = Uuid::parse_str(&payload.lesson_instance_id)
        .map_err(|_| ApiError::from_homework_domain(DomainError::LessonInstanceNotFound))?;
    let created_by = Uuid::parse_str(&payload.created_by)
        .map_err(|_| ApiError::from_homework_domain(DomainError::UserNotFound))?;
    let created_by_role = UserRole::from_str(&payload.created_by_role)
        .map_err(ApiError::from_homework_domain)?;
    let status = HomeworkStatus::from_str(&payload.status)
        .map_err(ApiError::from_homework_domain)?;

    let homework = create_homework_use_case
        .execute(CreateHomeworkCommand::new(
            lesson_instance_id,
            created_by,
            created_by_role,
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

async fn update_homework_text_handler(
    State(state): State<AppState>,
    Path(homework_id): Path<String>,
    Json(payload): Json<UpdateHomeworkRequest>,
) -> Result<Json<UpdateHomeworkResponse>, ApiError> {
    let update_homework_use_case = state.update_homework_use_case.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Update homework use case not initialized".to_string(),
    })?;

    let homework_uuid = Uuid::parse_str(&homework_id)
        .map_err(|_| ApiError::from_homework_domain(DomainError::HomeworkNotFound))?;

    let homework = update_homework_use_case
        .execute(UpdateHomeworkTextCommand::new(homework_uuid, payload.text_content))
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

#[derive(Debug, Deserialize)]
pub struct ClassScheduleWeekQuery {
    pub date: String,
    #[serde(default)]
    pub class_id: Option<String>,
}

async fn class_schedule_week_handler(
    State(state): State<AppState>,
    Query(query): Query<ClassScheduleWeekQuery>,
) -> Result<Json<ScheduleWeekResponse>, ApiError> {
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

async fn day_schedule_handler(
    State(state): State<AppState>,
    Query(query): Query<ScheduleDayQuery>,
) -> Result<Json<ScheduleDayResponseDto>, ApiError> {
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

#[derive(Debug, Deserialize)]
pub struct ClassScheduleDayQuery {
    pub date: String,
    #[serde(default)]
    pub class_id: Option<String>,
}

async fn class_day_schedule_handler(
    State(state): State<AppState>,
    Query(query): Query<ClassScheduleDayQuery>,
) -> Result<Json<ScheduleDayResponseDto>, ApiError> {
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

async fn classes_handler(
    State(state): State<AppState>,
) -> Result<Json<ClassListResponse>, ApiError> {
    let class_repository = state.class_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Class repository not initialized".to_string(),
    })?;

    let classes = class_repository
        .get_all_active()
        .await
        .map_err(ApiError::from_homework_domain)?;

    let response = ClassListResponse {
        classes: classes
            .into_iter()
            .map(|class| ClassListItemResponse {
                id: class.id.to_string(),
                graduation_year: class.graduation_year,
                letter: class.letter.to_string(),
                name: class.full_name(),
                is_active: class.is_active,
            })
            .collect(),
    };

    Ok(Json(response))
}

#[derive(Debug, Serialize)]
pub struct TelegramUserResponse {
    pub user_id: String,
    pub login: String,
    pub role: String,
    pub class_id: Option<String>,
}

/// Who is writing to the bot: Telegram id -> school user, only for an active binding
/// (made by the login flow in telegram-adapter). Used by the chat agent to set the homework author.
/// Requires `X-Agent-Key` equal to `AGENT_KEY`; without it the route answers 404 to everyone.
async fn telegram_user_handler(
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

#[derive(Debug, Deserialize)]
pub struct SendAuthCodeRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SendAuthCodeResponse {
    pub email: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyAuthCodeRequest {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyAuthCodeResponse {
    pub valid: bool,
    pub user_id: Option<String>,
}

async fn send_auth_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<SendAuthCodeRequest>,
) -> Result<Json<SendAuthCodeResponse>, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let one_time_code_service = state.one_time_code_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "One-time code service not initialized".to_string(),
    })?;

    let login = domain::value_objects::login::Login::from_email(&payload.email)
        .map_err(ApiError::from_auth_domain)?;
    let user = user_repository
        .get_by_login(login.as_str())
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    one_time_code_service
        .issue(user.id, &payload.email)
        .await
        .map_err(ApiError::from_auth_domain)?;

    Ok(Json(SendAuthCodeResponse {
        email: payload.email.trim().to_ascii_lowercase(),
        message: "Code sent to email".to_string(),
    }))
}

async fn verify_auth_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<VerifyAuthCodeRequest>,
) -> Result<Json<VerifyAuthCodeResponse>, ApiError> {
    let user_repository = state.user_repository.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "User repository not initialized".to_string(),
    })?;
    let one_time_code_service = state.one_time_code_service.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "One-time code service not initialized".to_string(),
    })?;

    let login = domain::value_objects::login::Login::from_email(&payload.email)
        .map_err(ApiError::from_auth_domain)?;
    let user = user_repository
        .get_by_login(login.as_str())
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !user.is_active {
        return Err(ApiError::from_auth_domain(DomainError::UserIsInactive));
    }

    let valid = one_time_code_service
        .verify(user.id, &payload.code)
        .await
        .map_err(ApiError::from_auth_domain)?;

    if !valid {
        return Err(ApiError::from_auth_domain(DomainError::InvalidAuthCode));
    }

    Ok(Json(VerifyAuthCodeResponse {
        valid: true,
        user_id: Some(user.id.to_string()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::use_cases::auth::login::LoginUseCase;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use domain::entities::user::User;
    use domain::repositories::user_repository::UserRepository;
    use domain::value_objects::role::UserRole;
    use tower::ServiceExt;
    use uuid::Uuid;

    struct MockUserRepository {
        user: Option<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_by_id(&self, _user_id: Uuid) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_active_students_by_class(
            &self,
            _class_id: Uuid,
        ) -> Result<Vec<User>, DomainError> {
            Ok(vec![])
        }

        async fn save(&self, _user: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    struct MockCredentialsStore {
        is_valid: bool,
    }

    #[async_trait::async_trait]
    impl domain::ports::auth::CredentialsStore for MockCredentialsStore {
        async fn verify_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<bool, DomainError> {
            Ok(self.is_valid)
        }

        async fn set_password(
            &self,
            _user_id: Uuid,
            _raw_password: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct MockTokenIssuer {
        token: String,
    }

    impl domain::ports::auth::TokenIssuer for MockTokenIssuer {
        fn issue(&self, _user: &User) -> Result<String, DomainError> {
            Ok(self.token.clone())
        }

        fn verify(&self, _token: &str) -> Result<domain::ports::auth::TokenClaims, DomainError> {
            unimplemented!()
        }
    }

    fn create_test_user() -> User {
        User::try_new(
            Uuid::new_v4(),
            "test_login".to_string(),
            UserRole::Student,
            "Ivanov".to_string(),
            "Ivan".to_string(),
            None,
            None,
        )
        .expect("Test user should be valid")
    }

    #[tokio::test]
    async fn health_route_works() {
        let app = create_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn login_route_works() {
        let user = create_test_user();
        let app = create_app_with_auth(Arc::new(LoginUseCase::new(
            Arc::new(MockUserRepository { user: Some(user.clone()) }),
            Arc::new(MockCredentialsStore { is_valid: true }),
            Arc::new(MockTokenIssuer {
                token: "mock_token".to_string(),
            }),
        )));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"login":"test_login","password":"correct_password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
