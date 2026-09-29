//! Device token entity used for push notification delivery.
//!
//! Device tokens are attached to a user and platform and are used by the
//! infrastructure notifier to send FCM/APNs payloads to active devices.

use crate::errors::DomainError;
use crate::value_objects::notification_platform::NotificationPlatform;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Push notification registration for a single device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceToken {
    /// Unique record identifier.
    pub id: Uuid,
    /// Owner of the device.
    pub user_id: Uuid,
    /// Platform of the device / provider.
    pub platform: NotificationPlatform,
    /// Push token issued by the provider (FCM/APNs).
    pub token: String,
    /// Whether the token is still active and should be used for sending.
    pub is_active: bool,
    /// Creation time.
    pub created_at: NaiveDateTime,
    /// Last update time.
    pub updated_at: NaiveDateTime,
}

impl DeviceToken {
    /// Creates a new device token record with validation.
    pub fn try_new(
        id: Uuid,
        user_id: Uuid,
        platform: NotificationPlatform,
        token: String,
        created_at: NaiveDateTime,
        updated_at: NaiveDateTime,
    ) -> Result<Self, DomainError> {
        let token = token.trim();
        if token.is_empty() || token.len() > 1024 {
            return Err(DomainError::InvalidDeviceTokenFormat);
        }

        Ok(Self {
            id,
            user_id,
            platform,
            token: token.to_string(),
            is_active: true,
            created_at,
            updated_at,
        })
    }

    /// Marks the token as inactive.
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    /// Reactivates the token after a client re-registers.
    pub fn activate(&mut self) {
        self.is_active = true;
    }
}
