//! Browser push notifications: VAPID key, (un)subscribing, and the background
//! notification after a homework change.
//!
//! Guarantees: a notification failure never fails the request that changed homework —
//! it runs in a spawned task and only logs.

use std::sync::Arc;

use application::use_cases::notifications::notify_homework_changed::{
    HomeworkChange, NotifyHomeworkChangedUseCase,
};
use application::use_cases::notifications::push_subscriptions::PushSubscriptionsUseCase;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use domain::errors::DomainError;
use domain::value_objects::web_push_subscription::WebPushSubscription;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::router::{require_authenticated_user, ApiError, AppState};

/// Everything push needs; `None` in `AppState` if `VAPID_PRIVATE_KEY` is not set.
#[derive(Clone)]
pub struct PushNotifications {
    /// For `pushManager.subscribe({applicationServerKey})`, base64url.
    pub public_key: String,
    pub subscriptions: Arc<PushSubscriptionsUseCase>,
    pub notifier: Arc<NotifyHomeworkChangedUseCase>,
}

#[derive(Debug, Serialize)]
pub struct VapidKeyResponse {
    pub public_key: String,
}

/// The browser's `PushSubscription.toJSON()` shape.
#[derive(Debug, Deserialize)]
pub struct SubscriptionRequest {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Serialize)]
pub struct UnsubscribeResponse {
    /// False if this browser was not subscribed.
    pub removed: bool,
}

fn push(state: &AppState) -> Result<&PushNotifications, ApiError> {
    state.push.as_ref().ok_or_else(|| ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        message: "Push notifications are not configured on this server".to_string(),
    })
}

fn subscription(payload: &SubscriptionRequest) -> Result<WebPushSubscription, ApiError> {
    WebPushSubscription::try_new(&payload.endpoint, &payload.keys.p256dh, &payload.keys.auth).map_err(|_| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        message: "Invalid push subscription".to_string(),
    })
}

fn internal(error: DomainError) -> ApiError {
    ApiError {
        status: if error == DomainError::UserNotFound {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        },
        message: error.to_string(),
    }
}

/// `GET /api/notifications/vapid-public-key` — public, the key is not a secret.
pub async fn vapid_public_key_handler(State(state): State<AppState>) -> Result<Json<VapidKeyResponse>, ApiError> {
    Ok(Json(VapidKeyResponse {
        public_key: push(&state)?.public_key.clone(),
    }))
}

/// `POST /api/notifications/subscriptions` — body is `PushSubscription.toJSON()`.
pub async fn subscribe_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubscriptionRequest>,
) -> Result<StatusCode, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    let push = push(&state)?;
    push.subscriptions
        .subscribe(user.id, &subscription(&payload)?)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/notifications/subscriptions` — same body; turns this browser off.
pub async fn unsubscribe_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubscriptionRequest>,
) -> Result<Json<UnsubscribeResponse>, ApiError> {
    let user = require_authenticated_user(&state, &headers).await?;
    let push = push(&state)?;
    let removed = push
        .subscriptions
        .unsubscribe(user.id, &subscription(&payload)?)
        .await
        .map_err(internal)?;
    Ok(Json(UnsubscribeResponse { removed }))
}

/// Notifies the lesson's students in the background; the author is skipped.
/// No-op if push is not configured.
pub(crate) fn notify_homework_changed(state: &AppState, homework_id: Uuid, change: HomeworkChange, author_id: Uuid) {
    spawn_notification(state, homework_id, None, change, author_id);
}

/// Same, for changes after which the homework may be gone (file removed, homework
/// deleted): recipients and text are resolved from the lesson.
pub(crate) fn notify_homework_change_for_lesson(
    state: &AppState,
    lesson_instance_id: Uuid,
    homework_id: Uuid,
    change: HomeworkChange,
    author_id: Uuid,
) {
    spawn_notification(state, homework_id, Some(lesson_instance_id), change, author_id);
}

fn spawn_notification(
    state: &AppState,
    homework_id: Uuid,
    lesson_instance_id: Option<Uuid>,
    change: HomeworkChange,
    author_id: Uuid,
) {
    let Some(push) = state.push.clone() else {
        return;
    };
    tokio::spawn(async move {
        let result = match lesson_instance_id {
            Some(lesson_instance_id) => {
                push.notifier
                    .execute_for_lesson(lesson_instance_id, homework_id, change, Some(author_id))
                    .await
            }
            None => push.notifier.execute(homework_id, change, Some(author_id)).await,
        };
        match result {
            Ok(summary) if summary.failed > 0 => eprintln!(
                "push: homework {homework_id}: {} of {} recipients failed",
                summary.failed, summary.recipients
            ),
            Ok(_) => {}
            Err(error) => eprintln!("push: homework {homework_id}: {error}"),
        }
    });
}
