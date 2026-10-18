//! Use case: update homework text.
//!
//! Flow:
//! 1. Validate the new text.
//! 2. Load the homework by id.
//! 3. Update the `text_content` field.
//! 4. Persist the updated homework.

use std::sync::Arc;

use domain::{
    entities::homework::Homework,
    errors::DomainError,
    repositories::homework_repository::HomeworkRepository,
};
use uuid::Uuid;

/// Command for updating homework text content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateHomeworkTextCommand {
    pub homework_id: Uuid,
    pub text_content: Option<String>,
}

impl UpdateHomeworkTextCommand {
    pub fn new(homework_id: Uuid, text_content: Option<String>) -> Self {
        Self {
            homework_id,
            text_content,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if let Some(text) = &self.text_content {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(DomainError::InvalidHomeworkTextFormat);
            }
        }

        Ok(())
    }
}

/// Use case that updates the text of an existing homework.
pub struct UpdateHomeworkTextUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
}

impl UpdateHomeworkTextUseCase {
    pub fn new(homeworks: Arc<dyn HomeworkRepository>) -> Self {
        Self { homeworks }
    }

    pub async fn execute(&self, cmd: UpdateHomeworkTextCommand) -> Result<Homework, DomainError> {
        cmd.validate()?;

        let mut homework = self.homeworks.get_by_id(cmd.homework_id).await?;
        homework.text_content = match cmd.text_content {
            Some(value) => Some(value.trim().to_string()),
            None => None,
        };

        self.homeworks.save(homework).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        entities::homework::{Homework, HomeworkFile},
        value_objects::{homework_status::HomeworkStatus, role::UserRole},
    };

    struct MockHomeworkRepository {
        homework: Homework,
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, _homework_id: Uuid) -> Result<Homework, DomainError> {
            Ok(self.homework.clone())
        }

        async fn get_by_lesson_instance(
            &self,
            _lesson_instance_id: Uuid,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, _homework_id: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            Ok(vec![])
        }

        async fn save(&self, homework: Homework) -> Result<Homework, DomainError> {
            Ok(homework)
        }

        async fn add_file(&self, _file: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn remove_file(&self, _file_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _homework_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_with_files(
            &self,
            homework: Homework,
            _files: Vec<HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            Ok(homework)
        }
    }

    #[test]
    fn test_validate_rejects_empty_text() {
        let cmd = UpdateHomeworkTextCommand::new(Uuid::new_v4(), Some("   ".to_string()));

        assert_eq!(cmd.validate(), Err(DomainError::InvalidHomeworkTextFormat));
    }

    #[test]
    fn test_validate_accepts_none() {
        let cmd = UpdateHomeworkTextCommand::new(Uuid::new_v4(), None);

        assert!(cmd.validate().is_ok());
    }

    #[tokio::test]
    async fn test_execute_updates_text() {
        let homework = Homework::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            UserRole::Teacher,
            Some("Old text".to_string()),
            HomeworkStatus::Draft,
            false,
            None,
            chrono::Utc::now(),
        )
        .unwrap();

        let repo = Arc::new(MockHomeworkRepository { homework: homework.clone() });
        let use_case = UpdateHomeworkTextUseCase::new(repo);

        let cmd = UpdateHomeworkTextCommand::new(homework.id, Some("New text".to_string()));
        let result = use_case.execute(cmd).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.text_content, Some("New text".to_string()));
    }
}
