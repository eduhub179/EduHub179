//! Course materials routes: courses of a class, topics, files and links.
//!
//! A course is a `lessons` row ("Матанализ — 2027в"); paths use `/api/courses/:lesson_id`
//! (`/api/lessons/:lesson_instance_id/...` is taken by homework files, and axum does not
//! allow two parameter names at one path position).
//! Access rules live in `MaterialsService`; handlers parse input and map DTOs.

use application::use_cases::homework::upload_file::UploadHomeworkFileCommand;
use application::use_cases::materials::materials_service::{
    AddLinkCommand, CourseSummary, MaterialsService, TopicsOverview, UpdateMaterialCommand,
    UploadMaterialCommand,
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use domain::{
    entities::{
        lesson_material::{LessonMaterial, LessonMaterialTopic, MaterialContent},
        user::User,
    },
    errors::DomainError,
    repositories::lesson_material_repository::MaterialTopicFilter,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};
use crate::routes::files::{file_response, multipart_error};

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Serialize)]
pub struct CourseResponse {
    pub lesson_id: String,
    pub subject_name: String,
    /// Class ("2027в") or group name.
    pub target_name: String,
    /// The current user may add / edit / delete materials of this course.
    pub can_manage: bool,
}

impl From<CourseSummary> for CourseResponse {
    fn from(course: CourseSummary) -> Self {
        Self {
            lesson_id: course.lesson_id.to_string(),
            subject_name: course.subject_name,
            target_name: course.target_name,
            can_manage: course.can_manage,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TopicResponse {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
    pub material_count: i64,
}

impl TopicResponse {
    fn new(topic: LessonMaterialTopic, material_count: i64) -> Self {
        Self {
            id: topic.id.to_string(),
            name: topic.name,
            sort_order: topic.sort_order,
            material_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TopicsResponse {
    pub topics: Vec<TopicResponse>,
    /// Materials without a topic ("Без темы").
    pub without_topic_count: i64,
    pub total_count: i64,
}

impl From<TopicsOverview> for TopicsResponse {
    fn from(overview: TopicsOverview) -> Self {
        Self {
            topics: overview
                .topics
                .into_iter()
                .map(|item| TopicResponse::new(item.topic, item.material_count))
                .collect(),
            without_topic_count: overview.without_topic_count,
            total_count: overview.total_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MaterialFileResponse {
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct MaterialResponse {
    pub id: String,
    pub lesson_id: String,
    /// null = "Без темы".
    pub topic_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    /// "file" | "link".
    pub kind: &'static str,
    /// For links.
    pub url: Option<String>,
    /// For files; contents via `GET /api/materials/{id}/file`.
    pub file: Option<MaterialFileResponse>,
    pub sort_order: i32,
    pub created_by: String,
    pub created_at: String,
}

impl From<LessonMaterial> for MaterialResponse {
    fn from(material: LessonMaterial) -> Self {
        let (kind, url, file) = match material.content {
            MaterialContent::Link { url } => ("link", Some(url), None),
            MaterialContent::File {
                file_name,
                mime_type,
                size_bytes,
                ..
            } => (
                "file",
                None,
                Some(MaterialFileResponse {
                    file_name,
                    mime_type,
                    size_bytes,
                }),
            ),
        };
        Self {
            id: material.id.to_string(),
            lesson_id: material.lesson_id.to_string(),
            topic_id: material.topic_id.map(|id| id.to_string()),
            title: material.title,
            description: material.description,
            kind,
            url,
            file,
            sort_order: material.sort_order,
            created_by: material.created_by.to_string(),
            created_at: material.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClassCoursesQuery {
    pub class_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MaterialsQuery {
    /// Absent: all materials; `none`: without a topic; a UUID: that topic.
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTopicRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct AddLinkRequest {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub topic_id: Option<String>,
}

/// Every field is optional; `null` clears `description` / moves to "Без темы" (`topic_id`).
#[derive(Debug, Deserialize)]
pub struct UpdateMaterialRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub topic_id: Option<Option<String>>,
    #[serde(default)]
    pub sort_order: Option<i32>,
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

async fn caller<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
) -> Result<(User, &'a MaterialsService), ApiError> {
    let user = require_authenticated_user(state, headers).await?;
    let service = state.materials_service.as_deref().ok_or_else(|| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Materials service not initialized".to_string(),
    })?;
    Ok((user, service))
}

fn parse_id(raw: &str, not_found: DomainError) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw.trim()).map_err(|_| ApiError::from_materials_domain(not_found))
}

fn parse_topic_id(raw: Option<&str>) -> Result<Option<Uuid>, ApiError> {
    raw.map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| parse_id(raw, DomainError::MaterialTopicNotFound))
        .transpose()
}

// ============================================================================
// Courses
// ============================================================================

/// `GET /api/courses?class_id=<uuid>` — courses of a class (staff: all, with `can_manage`;
/// student: only the courses they attend).
pub async fn class_courses_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ClassCoursesQuery>,
) -> Result<Json<Vec<CourseResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let class_id = parse_id(&query.class_id, DomainError::ClassNotFound)?;
    let courses = service
        .class_courses(&user, class_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(courses.into_iter().map(CourseResponse::from).collect()))
}

/// `GET /api/courses/:lesson_id` — one course (page header, `can_manage`).
pub async fn course_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CourseResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let course = service
        .course(&user, lesson_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(course.into()))
}

// ============================================================================
// Topics
// ============================================================================

/// `GET /api/courses/:lesson_id/material-topics` — topics with material counts.
pub async fn topics_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<TopicsResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let overview = service
        .topics(&user, lesson_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(overview.into()))
}

/// `POST /api/courses/:lesson_id/material-topics` `{"name": "..."}`.
pub async fn create_topic_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateTopicRequest>,
) -> Result<(StatusCode, Json<TopicResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let topic = service
        .create_topic(&user, lesson_id, &payload.name)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok((StatusCode::CREATED, Json(TopicResponse::new(topic, 0))))
}

/// `PATCH /api/material-topics/:topic_id` `{"name"?, "sort_order"?}`.
/// `material_count` in the answer is not recomputed (0); reload the topic list for counts.
pub async fn update_topic_handler(
    State(state): State<AppState>,
    Path(topic_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTopicRequest>,
) -> Result<Json<TopicResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let topic_id = parse_id(&topic_id, DomainError::MaterialTopicNotFound)?;
    let topic = service
        .update_topic(&user, topic_id, payload.name.as_deref(), payload.sort_order)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(TopicResponse::new(topic, 0)))
}

/// `DELETE /api/material-topics/:topic_id` — its materials move to "Без темы".
pub async fn delete_topic_handler(
    State(state): State<AppState>,
    Path(topic_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let topic_id = parse_id(&topic_id, DomainError::MaterialTopicNotFound)?;
    service
        .delete_topic(&user, topic_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Materials
// ============================================================================

/// `GET /api/courses/:lesson_id/materials[?topic=<uuid>|none]`.
pub async fn materials_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<MaterialsQuery>,
) -> Result<Json<Vec<MaterialResponse>>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;
    let filter = match query.topic.as_deref().map(str::trim) {
        None | Some("") => MaterialTopicFilter::All,
        Some("none") => MaterialTopicFilter::WithoutTopic,
        Some(raw) => MaterialTopicFilter::Topic(parse_id(raw, DomainError::MaterialTopicNotFound)?),
    };
    let materials = service
        .materials(&user, lesson_id, filter)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(materials.into_iter().map(MaterialResponse::from).collect()))
}

/// `POST /api/courses/:lesson_id/materials/links` `{"title", "url", "description"?, "topic_id"?}`.
pub async fn add_link_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AddLinkRequest>,
) -> Result<(StatusCode, Json<MaterialResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let command = AddLinkCommand {
        lesson_id: parse_id(&lesson_id, DomainError::LessonNotFound)?,
        topic_id: parse_topic_id(payload.topic_id.as_deref())?,
        title: payload.title,
        url: payload.url,
        description: payload.description,
    };
    let material = service
        .add_link(&user, command)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok((StatusCode::CREATED, Json(material.into())))
}

/// `POST /api/courses/:lesson_id/materials/files` — multipart: `file` (required),
/// `title`, `description`, `topic_id` (optional text fields). Title defaults to the file name.
pub async fn upload_file_handler(
    State(state): State<AppState>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MaterialResponse>), ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let lesson_id = parse_id(&lesson_id, DomainError::LessonNotFound)?;

    let (mut file, mut title, mut description, mut topic_id) = (None, None, None, None);
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        match field.name().unwrap_or_default() {
            "file" => {
                let file_name = field.file_name().unwrap_or_default().to_string();
                let mime_type = field.content_type().unwrap_or("application/octet-stream").to_string();
                let bytes = field.bytes().await.map_err(multipart_error)?;
                file = Some(UploadHomeworkFileCommand::new(file_name, mime_type, bytes.to_vec()));
            }
            "title" => title = Some(field.text().await.map_err(multipart_error)?),
            "description" => description = Some(field.text().await.map_err(multipart_error)?),
            "topic_id" => topic_id = Some(field.text().await.map_err(multipart_error)?),
            _ => {}
        }
    }
    let file = file.ok_or_else(|| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        message: "Multipart field 'file' is required".to_string(),
    })?;

    let material = service
        .upload_file(
            &user,
            UploadMaterialCommand {
                lesson_id,
                topic_id: parse_topic_id(topic_id.as_deref())?,
                title,
                description,
                file,
            },
        )
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok((StatusCode::CREATED, Json(material.into())))
}

/// `PATCH /api/materials/:material_id` `{"title"?, "description"?, "topic_id"?, "sort_order"?}`.
pub async fn update_material_handler(
    State(state): State<AppState>,
    Path(material_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateMaterialRequest>,
) -> Result<Json<MaterialResponse>, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let material_id = parse_id(&material_id, DomainError::MaterialNotFound)?;
    let topic_id = match payload.topic_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => Some(Some(parse_id(&raw, DomainError::MaterialTopicNotFound)?)),
    };
    let material = service
        .update_material(
            &user,
            material_id,
            UpdateMaterialCommand {
                title: payload.title,
                description: payload.description,
                topic_id,
                sort_order: payload.sort_order,
            },
        )
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(Json(material.into()))
}

/// `DELETE /api/materials/:material_id` — removes the record and its file.
pub async fn delete_material_handler(
    State(state): State<AppState>,
    Path(material_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let material_id = parse_id(&material_id, DomainError::MaterialNotFound)?;
    let deleted = service
        .delete_material(&user, material_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    if let Some(key) = deleted.orphaned_storage_key {
        // The record is gone, the object stayed: leave a trace for manual cleanup.
        eprintln!("file storage: could not delete object '{key}' (material {material_id} removed)");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/materials/:material_id/file` — file contents (cached by the browser).
pub async fn download_material_handler(
    State(state): State<AppState>,
    Path(material_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (user, service) = caller(&state, &headers).await?;
    let material_id = parse_id(&material_id, DomainError::MaterialNotFound)?;
    let downloaded = service
        .download(&user, material_id)
        .await
        .map_err(ApiError::from_materials_domain)?;
    Ok(file_response(downloaded.bytes, &downloaded.file_name, &downloaded.mime_type))
}
