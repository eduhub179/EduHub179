use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::router::{ApiError, AppState};

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

pub async fn classes_handler(
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
