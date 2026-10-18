//! Real push sender adapter stub.
//!
//! NOTE: This is NOT the final production implementation.
//! - It is a placeholder that implements the `NotificationSender` trait so the
//!   application compiles and wiring can be tested locally.
//! - To use real push delivery, replace the internals of `send_to_device` and
//!   `send_to_user` with calls to a concrete provider (FCM, APNs, OneSignal, etc).
//! - Store provider credentials in environment variables or a secret manager,
//!   do not hardcode them here.
//! - Add retry, error mapping to `DomainError`, and tests that exercise failure
//!   cases (invalid tokens, provider errors, rate limits).
//!
//! For unit tests, provide a separate mock implementation (in tests or a
//! dedicated `mocks` module) rather than modifying this file.

use domain::errors::DomainError;
use domain::ports::notification_sender::{NotificationPayload, NotificationSender};
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use std::sync::Arc;
use uuid::Uuid;

pub struct PushSender {
    device_tokens: Arc<dyn DeviceTokenRepository>,
}

impl PushSender {
    pub fn new(device_tokens: Arc<dyn DeviceTokenRepository>) -> Self {
        Self { device_tokens }
    }
}

#[async_trait::async_trait]
impl NotificationSender for PushSender {
    async fn send_to_device(
        &self,
        token: &str,
        platform: NotificationPlatform,
        payload: &NotificationPayload,
    ) -> Result<(), DomainError> {
        let token = token.trim();
        if token.is_empty() || token.len() > 1024 {
            return Err(DomainError::InvalidDeviceTokenFormat);
        }

        // This is the real provider boundary. In a production implementation this
        // method would call FCM/APNs/OneSignal HTTP APIs. For now we treat it as a
        // successful dispatch step after validation and payload preparation.
        let _ = (platform, payload, token);
        Ok(())
    }

    async fn send_to_user(
        &self,
        user_id: Uuid,
        payload: &NotificationPayload,
    ) -> Result<(), DomainError> {
        let active_tokens = self.device_tokens.get_active_by_user(user_id).await?;

        for device_token in active_tokens {
            self.send_to_device(&device_token.token, device_token.platform, payload).await?;
        }

        Ok(())
    }
}
