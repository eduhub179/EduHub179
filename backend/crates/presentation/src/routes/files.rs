//! Homework file routes: attach a file to a lesson, download it, delete it.
//!
//! Files belong to the homework of a lesson instance (one homework per lesson);
//! the bytes live in object storage (MinIO), the DB keeps only metadata.
//!
//! Guarantees: no panics; storage keys never come from the client.

use axum::{
    body::Body,
    extract::{multipart::MultipartError, Multipart, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use application::use_cases::homework::{
    attach_file_to_lesson::AttachFileToLessonCommand, delete_file::DeleteHomeworkFileCommand,
    upload_file::UploadHomeworkFileCommand,
};
use application::use_cases::notifications::notify_homework_changed::HomeworkChange;
use domain::{entities::homework::HomeworkFile, errors::DomainError};
use serde::Serialize;
use uuid::Uuid;

use crate::routes::notifications::{notify_homework_change_for_lesson, notify_homework_changed};
use crate::router::{can_edit_homework, require_authenticated_user, require_caller, ApiError, AppState, Caller, FileUseCases};

/// Multipart field with the file.
const FILE_FIELD: &str = "file";

/// Types the browser may render inline. Anything else (HTML, SVG, ...) is sent as an
/// attachment: an uploaded HTML page must not run as a page of the API origin.
const INLINE_MIME_TYPES: &[&str] = &[
    "application/pdf",
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
];

#[derive(Debug, Serialize)]
pub struct HomeworkFileResponse {
    pub id: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sort_order: i32,
}

impl From<HomeworkFile> for HomeworkFileResponse {
    fn from(file: HomeworkFile) -> Self {
        Self {
            id: file.id.to_string(),
            file_name: file.file_name,
            mime_type: file.mime_type,
            size_bytes: file.size_bytes,
            sort_order: file.sort_order,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AttachFileResponse {
    pub homework_id: String,
    /// True if the lesson had no homework and it was created for this file.
    pub homework_created: bool,
    pub file: HomeworkFileResponse,
}

#[derive(Debug, Serialize)]
pub struct DeleteFileResponse {
    /// True if the homework had no text and this was its last file, so it was deleted too.
    pub homework_deleted: bool,
}

fn file_use_cases(state: &AppState) -> Result<&FileUseCases, ApiError> {
    state.file_use_cases.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        message: "File storage is not configured".to_string(),
    })
}

fn parse_id(raw: &str, not_found: DomainError) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::from_homework_domain(not_found))
}

/// Teachers, admins and the agent manage homework files, the same rule as for homework text.
async fn require_file_editor(state: &AppState, headers: &HeaderMap) -> Result<Caller, ApiError> {
    let caller = require_caller(state, headers).await?;
    if !can_edit_homework(&caller.user, caller.is_agent) {
        return Err(ApiError::from_homework_domain(DomainError::InsufficientPermissions));
    }
    Ok(caller)
}

pub(crate) fn multipart_error(error: MultipartError) -> ApiError {
    ApiError {
        // 413 when the body limit is hit, 400 for malformed multipart.
        status: error.status(),
        message: error.body_text(),
    }
}

/// `POST /api/lessons/:lesson_instance_id/files` — multipart with field `file`.
///
/// Creates the lesson's homework if it has none (file-only homework).
pub async fn attach_file_to_lesson_handler(
    State(state): State<AppState>,
    Path(lesson_instance_id): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<AttachFileResponse>, ApiError> {
    let Caller { user, .. } = require_file_editor(&state, &headers).await?;
    let use_cases = file_use_cases(&state)?;
    let lesson_instance_id = parse_id(&lesson_instance_id, DomainError::LessonInstanceNotFound)?;

    let mut upload = None;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some(FILE_FIELD) {
            continue;
        }
        let file_name = field.file_name().unwrap_or_default().to_string();
        let mime_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = field.bytes().await.map_err(multipart_error)?;
        upload = Some(UploadHomeworkFileCommand::new(file_name, mime_type, bytes.to_vec()));
        break;
    }
    let file = upload.ok_or_else(|| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        message: format!("Multipart field '{FILE_FIELD}' is required"),
    })?;

    let attached = use_cases
        .attach_to_lesson
        .execute(AttachFileToLessonCommand {
            lesson_instance_id,
            uploaded_by: user.id,
            uploaded_by_role: user.role,
            file,
        })
        .await
        .map_err(ApiError::from_homework_domain)?;

    let change = if attached.homework_created {
        HomeworkChange::Created
    } else {
        HomeworkChange::FileAdded {
            file_name: attached.file.file_name.clone(),
        }
    };
    notify_homework_changed(&state, attached.homework_id, change, user.id);

    Ok(Json(AttachFileResponse {
        homework_id: attached.homework_id.to_string(),
        homework_created: attached.homework_created,
        file: attached.file.into(),
    }))
}

/// `GET /api/homework/:homework_id/files/:file_id` — file contents, for any logged-in user
/// (homework is visible to the whole class, so are its files).
pub async fn download_homework_file_handler(
    State(state): State<AppState>,
    Path((homework_id, file_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_authenticated_user(&state, &headers).await?;
    let use_cases = file_use_cases(&state)?;
    let homework_id = parse_id(&homework_id, DomainError::HomeworkNotFound)?;
    let file_id = parse_id(&file_id, DomainError::HomeworkFileNotFound)?;

    let downloaded = use_cases
        .download
        .execute(homework_id, file_id)
        .await
        .map_err(ApiError::from_homework_domain)?;

    Ok(file_response(
        downloaded.bytes,
        &downloaded.file.file_name,
        &downloaded.file.mime_type,
    ))
}

/// A stored file as an HTTP response, shared by homework files and course materials.
///
/// - PDF and images open in the browser, everything else downloads as an attachment;
/// - `nosniff`, and `CSP: sandbox` for attachments: an uploaded HTML page cannot run
///   scripts in the API origin;
/// - `Cache-Control: private, immutable`: a stored file never changes (a new upload is a
///   new key and a new id), so the browser keeps it for a year and does not ask again.
pub(crate) fn file_response(bytes: Vec<u8>, file_name: &str, mime_type: &str) -> Response {
    let inline = INLINE_MIME_TYPES.contains(&mime_type);
    let content_type =
        HeaderValue::from_str(mime_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));

    let mut response = Response::new(Body::from(bytes));
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, content_type);
    response_headers.insert(header::CONTENT_DISPOSITION, content_disposition(file_name, inline));
    response_headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    if !inline {
        // Not for inline types: `sandbox` blocks the browser's built-in PDF viewer,
        // and PDFs/images cannot run page scripts anyway.
        response_headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox"));
    }
    response
}

/// `DELETE /api/homework/files/:file_id` — removes the file from the homework and storage
/// (and the homework itself if it had no text and this was its last file).
pub async fn delete_homework_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<DeleteFileResponse>, ApiError> {
    let Caller { user, .. } = require_file_editor(&state, &headers).await?;
    let use_cases = file_use_cases(&state)?;
    let file_id = parse_id(&file_id, DomainError::HomeworkFileNotFound)?;

    let deleted = use_cases
        .delete
        .execute(DeleteHomeworkFileCommand { file_id })
        .await
        .map_err(ApiError::from_homework_domain)?;
    if let Some(key) = deleted.orphaned_storage_key {
        // The record is gone, the object stayed: leave a trace for manual cleanup.
        eprintln!("file storage: could not delete object '{key}' (record {file_id} removed)");
    }
    if let Some(lesson_instance_id) = deleted.lesson_instance_id {
        let change = if deleted.homework_deleted {
            HomeworkChange::Deleted
        } else {
            HomeworkChange::FileRemoved {
                file_name: deleted.file.file_name.clone(),
            }
        };
        notify_homework_change_for_lesson(&state, lesson_instance_id, deleted.file.homework_id, change, user.id);
    }

    Ok(Json(DeleteFileResponse {
        homework_deleted: deleted.homework_deleted,
    }))
}

/// `inline` / `attachment` with the original name: an ASCII fallback for old clients
/// and the exact UTF-8 name in `filename*` (RFC 6266 / RFC 5987), e.g. «листок 1.pdf».
fn content_disposition(file_name: &str, inline: bool) -> HeaderValue {
    let ascii_name: String = file_name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' => c,
            _ => '_',
        })
        .collect();
    let encoded: String = file_name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let kind = if inline { "inline" } else { "attachment" };

    HeaderValue::from_str(&format!(
        "{kind}; filename=\"{ascii_name}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or(HeaderValue::from_static("attachment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_keeps_utf8_name() {
        let value = content_disposition("листок 1.pdf", true);

        assert_eq!(
            value.to_str().unwrap(),
            "inline; filename=\"______ 1.pdf\"; filename*=UTF-8''%D0%BB%D0%B8%D1%81%D1%82%D0%BE%D0%BA%201.pdf"
        );
    }

    #[test]
    fn content_disposition_escapes_quotes() {
        let value = content_disposition("a\"b.html", false);

        assert_eq!(
            value.to_str().unwrap(),
            "attachment; filename=\"a_b.html\"; filename*=UTF-8''a%22b.html"
        );
    }
}
