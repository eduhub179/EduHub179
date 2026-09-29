//! Use case: notify users about a newly created or updated homework.
//!
//! Triggering rule:
//! - called when homework is created or updated
//! - recipients are the users who should be notified about this homework
//! - the actual delivery is delegated to `NotificationSender`
//!
//! This is intentionally domain-agnostic: it does not know whether the sender is
//! FCM, APNs, OneSignal or something else.

use domain::errors::DomainError;
use domain::ports::notification_sender::NotificationPayload;
use domain::ports::notification_sender::NotificationSender;
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::repositories::user_repository::UserRepository;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NotifyNewHomeworkCommand {
    pub homework_id: Uuid,
    pub lesson_instance_id: Uuid,
    pub title: String,
    pub body: String,
    pub recipient_user_ids: Vec<Uuid>,
}

impl NotifyNewHomeworkCommand {
    pub fn new(
        homework_id: Uuid,
        lesson_instance_id: Uuid,
        title: String,
        body: String,
        recipient_user_ids: Vec<Uuid>,
    ) -> Self {
        Self {
            homework_id,
            lesson_instance_id,
            title,
            body,
            recipient_user_ids,
        }
    }
}

pub struct NotifyNewHomeworkUseCase {
    users: Arc<dyn UserRepository>,
    device_tokens: Arc<dyn DeviceTokenRepository>,
    sender: Arc<dyn NotificationSender>,
}

impl NotifyNewHomeworkUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        device_tokens: Arc<dyn DeviceTokenRepository>,
        sender: Arc<dyn NotificationSender>,
    ) -> Self {
        Self {
            users,
            device_tokens,
            sender,
        }
    }

    pub async fn execute(&self, cmd: NotifyNewHomeworkCommand) -> Result<(), DomainError> {
        if cmd.recipient_user_ids.is_empty() {
            return Ok(());
        }

        for user_id in &cmd.recipient_user_ids {
            self.users.get_by_id(*user_id).await?;

            let mut data = HashMap::new();
            data.insert("homework_id".to_string(), cmd.homework_id.to_string());
            data.insert(
                "lesson_instance_id".to_string(),
                cmd.lesson_instance_id.to_string(),
            );

            let payload = NotificationPayload {
                user_id: *user_id,
                title: cmd.title.trim().to_string(),
                body: cmd.body.trim().to_string(),
                data,
            };

            self.sender.send_to_user(*user_id, &payload).await?;
        }

        // `device_tokens` is kept as a dependency so later this use case can also
        // query active devices, store a notification log or filter recipients by
        // token availability before sending.
        let _ = &self.device_tokens;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::device_token::DeviceToken;
    use domain::entities::user::User;
    use domain::ports::notification_sender::NotificationPayload;
    use domain::repositories::device_token_repository::DeviceTokenRepository;
    use domain::value_objects::notification_platform::NotificationPlatform;
    use domain::value_objects::role::UserRole;

    struct MockUserRepo {
        users: Vec<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepo {
        async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError> {
            self.users
                .iter()
                .find(|user| user.id == user_id)
                .cloned()
                .ok_or(DomainError::UserNotFound)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            unimplemented!()
        }

        async fn get_active_students_by_class(&self, _class_id: Uuid) -> Result<Vec<User>, DomainError> {
            Ok(Vec::new())
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

        async fn get_active_by_user(&self, _user_id: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
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

    struct MockNotificationSender {
        sent: std::sync::Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl NotificationSender for MockNotificationSender {
        async fn send_to_device(
            &self,
            _token: &str,
            _platform: NotificationPlatform,
            _payload: &NotificationPayload,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn send_to_user(
            &self,
            user_id: Uuid,
            payload: &NotificationPayload,
        ) -> Result<(), DomainError> {
            assert_eq!(payload.user_id, user_id);
            self.sent.lock().unwrap().push(user_id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_notify_new_homework_success_for_multiple_recipients() {
        let user_1 = User::try_new(
            Uuid::new_v4(),
            "student_1".to_string(),
            UserRole::Student,
            "Petrov".to_string(),
            "Ivan".to_string(),
            None,
            None,
        )
        .unwrap();

        let user_2 = User::try_new(
            Uuid::new_v4(),
            "student_2".to_string(),
            UserRole::Student,
            "Sidorov".to_string(),
            "Petr".to_string(),
            None,
            None,
        )
        .unwrap();

        let sender = Arc::new(MockNotificationSender {
            sent: std::sync::Mutex::new(Vec::new()),
        });

        let use_case = NotifyNewHomeworkUseCase::new(
            Arc::new(MockUserRepo { users: vec![user_1.clone(), user_2.clone()] }),
            Arc::new(MockDeviceTokenRepo),
            sender.clone(),
        );

        let cmd = NotifyNewHomeworkCommand::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "New homework".to_string(),
            "Check the assignment".to_string(),
            vec![user_1.id, user_2.id],
        );

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        assert_eq!(sender.sent.lock().unwrap().len(), 2);
    }
}
