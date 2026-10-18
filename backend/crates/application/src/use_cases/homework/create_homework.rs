//! Use case: create a homework assignment.
//!
//! Flow:
//! 1. Validate the command parameters.
//! 2. Build a domain `Homework` entity.
//! 3. Persist it via `HomeworkRepository::save`.
//!
//! This keeps the domain model independent from PostgreSQL details while
//! still enforcing homework creation rules at the application layer.

use std::sync::Arc;

use chrono::Utc;
use domain::{
    entities::homework::Homework,
    errors::DomainError,
    repositories::homework_repository::HomeworkRepository,
    value_objects::{homework_status::HomeworkStatus, role::UserRole},
};
use uuid::Uuid;

/// Command for creating a homework record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateHomeworkCommand {
    /// Concrete lesson instance this homework belongs to.
    pub lesson_instance_id: Uuid,
    /// User who created the homework.
    pub created_by: Uuid,
    /// Role of the creator at the moment of creation.
    pub created_by_role: UserRole,
    /// Optional textual assignment content.
    pub text_content: Option<String>,
    /// Homework status.
    pub status: HomeworkStatus,
}

impl CreateHomeworkCommand {
    pub fn new(
        lesson_instance_id: Uuid,
        created_by: Uuid,
        created_by_role: UserRole,
        text_content: Option<String>,
        status: HomeworkStatus,
    ) -> Self {
        Self {
            lesson_instance_id,
            created_by,
            created_by_role,
            text_content,
            status,
        }
    }

    /// Validates command-level invariants before persisting.
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

/// Use case that creates a homework assignment.
pub struct CreateHomeworkUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
}

impl CreateHomeworkUseCase {
    pub fn new(homeworks: Arc<dyn HomeworkRepository>) -> Self {
        Self { homeworks }
    }

    /// Creates a new homework record.
    pub async fn execute(&self, cmd: CreateHomeworkCommand) -> Result<Homework, DomainError> {
        cmd.validate()?;

        let homework = Homework::try_new(
            Uuid::new_v4(),
            cmd.lesson_instance_id,
            cmd.created_by,
            cmd.created_by_role,
            cmd.text_content,
            cmd.status,
            false,
            None,
            Utc::now(),
        )?;

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

    struct MockHomeworkRepository;

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, _homework_id: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
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
        let cmd = CreateHomeworkCommand::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            UserRole::Teacher,
            Some("   ".to_string()),
            HomeworkStatus::Draft,
        );

        assert_eq!(cmd.validate(), Err(DomainError::InvalidHomeworkTextFormat));
    }

    #[test]
    fn test_validate_accepts_empty_text() {
        let cmd = CreateHomeworkCommand::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            UserRole::Teacher,
            None,
            HomeworkStatus::Draft,
        );

        assert!(cmd.validate().is_ok());
    }

    #[tokio::test]
    async fn test_execute_success() {
        let repo = Arc::new(MockHomeworkRepository);
        let use_case = CreateHomeworkUseCase::new(repo);

        let cmd = CreateHomeworkCommand::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            UserRole::Teacher,
            Some("Solve tasks 1-5".to_string()),
            HomeworkStatus::Draft,
        );

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
    }
}
