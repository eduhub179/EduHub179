//! Use case: turn browser push notifications on / off for the current user.
//!
//! A subscription is stored as a `device_tokens` row (platform `web`). Subscribing twice
//! from the same browser is idempotent (upsert on user + platform + token).
//!
//! Dependencies: `DeviceTokenRepository`, `WebPushSubscription`.
//! Guarantees: returns `Result`, no panics.

use std::sync::Arc;

use chrono::Utc;
use domain::entities::device_token::DeviceToken;
use domain::errors::DomainError;
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use domain::value_objects::web_push_subscription::WebPushSubscription;
use uuid::Uuid;

pub struct PushSubscriptionsUseCase {
    device_tokens: Arc<dyn DeviceTokenRepository>,
}

impl PushSubscriptionsUseCase {
    pub fn new(device_tokens: Arc<dyn DeviceTokenRepository>) -> Self {
        Self { device_tokens }
    }

    /// Stores (or re-activates) the browser subscription of the user and unlinks the same
    /// browser from anyone else: a browser delivers notifications of one user only — the
    /// one who linked it last (covers sessions that expired without a logout).
    pub async fn subscribe(&self, user_id: Uuid, subscription: &WebPushSubscription) -> Result<(), DomainError> {
        let now = Utc::now().naive_utc();
        let token = DeviceToken::try_new(
            Uuid::new_v4(),
            user_id,
            NotificationPlatform::Web,
            subscription.to_token(),
            now,
            now,
        )?;
        let token = self.device_tokens.save(token).await?;
        self.device_tokens
            .deactivate_token_for_other_users(NotificationPlatform::Web, &token.token, user_id)
            .await
    }

    /// Deactivates this browser's subscription of the user (turning notifications off,
    /// logging out). Returns false if it was not active.
    pub async fn unsubscribe(&self, user_id: Uuid, subscription: &WebPushSubscription) -> Result<bool, DomainError> {
        let token = subscription.to_token();
        let mut found = false;
        for device in self
            .device_tokens
            .get_active_by_user_and_platform(user_id, NotificationPlatform::Web)
            .await?
        {
            if device.token == token {
                self.device_tokens.deactivate(device.id).await?;
                found = true;
            }
        }
        Ok(found)
    }
}
