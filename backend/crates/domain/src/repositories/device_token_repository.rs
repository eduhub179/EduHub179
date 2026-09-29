//! Repository trait for device-token persistence.
//!
//! Dependencies: Only types from `crate::entities`, `crate::errors`,
//! `crate::value_objects`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::device_token::DeviceToken;
use crate::errors::DomainError;
use crate::value_objects::notification_platform::NotificationPlatform;
use uuid::Uuid;

/// Interface for working with push-device registrations.
///
/// A device token is not a user identity: multiple tokens can belong to the
/// same user, and the same token can be replaced when the app is reinstalled.
/// This repository is responsible for storing the active delivery channels for
/// push notifications.
#[async_trait::async_trait]
pub trait DeviceTokenRepository: Send + Sync {
    /// Fetches one device token by its unique database identifier.
    async fn get_by_id(&self, device_token_id: Uuid) -> Result<DeviceToken, DomainError>;

    /// Fetches all tokens for a user in the order of last update.
    async fn get_by_user(&self, user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError>;

    /// Fetches only active tokens for a user.
    ///
    /// This is the main query used by the push sender when a homework or event
    /// triggers a notification for that user.
    async fn get_active_by_user(&self, user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError>;

    /// Fetches all active tokens for a user on a concrete platform.
    async fn get_active_by_user_and_platform(
        &self,
        user_id: Uuid,
        platform: NotificationPlatform,
    ) -> Result<Vec<DeviceToken>, DomainError>;

    /// Saves or updates a device token.
    ///
    /// Usually used for upsert on `(user_id, platform, token)`. If the same
    /// physical token is re-registered, the row is updated instead of duplicated.
    async fn save(&self, device_token: DeviceToken) -> Result<DeviceToken, DomainError>;

    /// Marks a token as inactive.
    ///
    /// This is used when the client logs out, the device is removed, or the
    /// provider invalidates the token.
    async fn deactivate(&self, device_token_id: Uuid) -> Result<(), DomainError>;

    /// Deactivates all tokens for a user.
    ///
    /// Useful when the user logs out everywhere or explicitly clears devices.
    async fn deactivate_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError>;
}
