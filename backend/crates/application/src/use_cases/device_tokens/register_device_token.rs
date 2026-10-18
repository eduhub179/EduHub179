//! Use case: register an active push token for a user.
//!
//! Flow:
//! 1. Check that the user exists.
//! 2. Validate platform and token.
//! 3. Upsert the token for the user + platform.
//! 4. Mark previous device tokens as inactive if the client explicitly clears them.

use chrono::Utc;
use domain::entities::device_token::DeviceToken;
use domain::errors::DomainError;
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RegisterDeviceTokenCommand {
    pub user_id: Uuid,
    pub platform: String,
    pub token: String,
}

impl RegisterDeviceTokenCommand {
    pub fn try_new(user_id: Uuid, platform: String, token: String) -> Result<Self, DomainError> {
        let platform = NotificationPlatform::from_str(&platform)?;
        let token = token.trim();
        if token.is_empty() || token.len() > 1024 {
            return Err(DomainError::InvalidDeviceTokenFormat);
        }

        // Keep the runtime value in the canonical lower-case form.
        let _ = platform;

        Ok(Self {
            user_id,
            platform: platform.to_string(),
            token: token.to_string(),
        })
    }
}

pub struct RegisterDeviceTokenUseCase {
    users: Arc<dyn UserRepository>,
    device_tokens: Arc<dyn DeviceTokenRepository>,
}

impl RegisterDeviceTokenUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        device_tokens: Arc<dyn DeviceTokenRepository>,
    ) -> Self {
        Self {
            users,
            device_tokens,
        }
    }

    pub async fn execute(
        &self,
        cmd: RegisterDeviceTokenCommand,
    ) -> Result<DeviceToken, DomainError> {
        // 1. Ensure the user exists.
        self.users.get_by_id(cmd.user_id).await?;

        // 2. Parse platform to domain value object.
        let platform = NotificationPlatform::from_str(&cmd.platform)?;

        // 3. Reject empty token values.
        let token = cmd.token.trim();
        if token.is_empty() || token.len() > 1024 {
            return Err(DomainError::InvalidDeviceTokenFormat);
        }

        let now = Utc::now().naive_utc();
        let entity = DeviceToken::try_new(
            Uuid::new_v4(),
            cmd.user_id,
            platform,
            token.to_string(),
            now,
            now,
        )?;

        // 4. Upsert the token for the user/platform.
        self.device_tokens.save(entity.clone()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::user::User;
    use domain::value_objects::role::UserRole;

    struct MockUserRepo {
        user: Option<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepo {
        async fn get_by_id(&self, _user_id: Uuid) -> Result<User, DomainError> {
            self.user.clone().ok_or(DomainError::UserNotFound)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            unimplemented!()
        }

        async fn get_active_students_by_class(
            &self,
            _class_id: Uuid,
        ) -> Result<Vec<User>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _user: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    struct MockDeviceTokenRepo;

    #[async_trait::async_trait]
    impl DeviceTokenRepository for MockDeviceTokenRepo {
        async fn get_by_id(&self, _device_token_id: Uuid) -> Result<DeviceToken, DomainError> {
            unimplemented!()
        }

        async fn get_by_user(&self, _user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_active_by_user(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<DeviceToken>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_active_by_user_and_platform(
            &self,
            _user_id: Uuid,
            _platform: NotificationPlatform,
        ) -> Result<Vec<DeviceToken>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, device_token: DeviceToken) -> Result<DeviceToken, DomainError> {
            Ok(device_token)
        }

        async fn deactivate(&self, _device_token_id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }

        async fn deactivate_all_for_user(&self, _user_id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_register_device_token_success() {
        let user = User::try_new(
            Uuid::new_v4(),
            "student_1".to_string(),
            UserRole::Student,
            "Petrov".to_string(),
            "Ivan".to_string(),
            None,
            None,
        )
        .unwrap();

        let use_case = RegisterDeviceTokenUseCase::new(
            Arc::new(MockUserRepo {
                user: Some(user.clone()),
            }),
            Arc::new(MockDeviceTokenRepo),
        );

        let cmd = RegisterDeviceTokenCommand::try_new(
            user.id,
            "android".to_string(),
            "test-device-token".to_string(),
        )
        .unwrap();

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        let saved = result.unwrap();
        assert_eq!(saved.user_id, user.id);
        assert_eq!(saved.platform, NotificationPlatform::Android);
    }
}
