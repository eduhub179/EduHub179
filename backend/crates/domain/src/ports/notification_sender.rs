//! Notification ports: push delivery for homework and moderation events.
//!
//! Implemented by the `infrastructure` crate (FCM/APNs/OneSignal adapters).
//! Consumed by application use cases that need to notify users.

use crate::errors::DomainError;
use crate::value_objects::notification_platform::NotificationPlatform;
use std::collections::HashMap;
use uuid::Uuid;

/// Payload sent to one or many device tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPayload {
    /// Recipient user whose devices should receive the notification.
    pub user_id: Uuid,
    /// Short title shown by the client.
    pub title: String,
    /// Body text shown by the client.
    pub body: String,
    /// Optional structured metadata for the app.
    pub data: HashMap<String, String>,
}

/// Push notification sender interface.
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    /// Sends a push notification to a specific device token.
    async fn send_to_device(
        &self,
        token: &str,
        platform: NotificationPlatform,
        payload: &NotificationPayload,
    ) -> Result<(), DomainError>;

    /// Sends the same notification to all active device tokens for a user.
    async fn send_to_user(
        &self,
        user_id: Uuid,
        payload: &NotificationPayload,
    ) -> Result<(), DomainError>;
}
