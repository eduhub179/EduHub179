//! Use case: attach an uploaded file to a homework.
//!
//! Flow:
//! 1. Validate file metadata (`HomeworkFile::try_new`).
//! 2. Ensure the target homework exists.
//! 3. Persist the file record via `HomeworkRepository::add_file`.
//!
//! This keeps object storage and PostgreSQL concerns separate:
//! - the actual bytes live in `FileStorage`
//! - the DB stores only metadata via `HomeworkRepository`

use std::sync::Arc;

use domain::entities::homework::HomeworkFile;
use domain::errors::DomainError;
use domain::repositories::homework_repository::HomeworkRepository;
use uuid::Uuid;

/// Input for attaching a stored file to a homework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachHomeworkFileCommand {
    /// Target homework this file belongs to.
    pub homework_id: Uuid,
    /// Optional explicit file id. If `None`, a new UUID is generated.
    pub file_id: Option<Uuid>,
    /// Storage key provided by the object store.
    pub storage_key: String,
    /// Original file name as displayed to the user.
    pub file_name: String,
    /// MIME type reported by the client or detected by the file service.
    pub mime_type: String,
    /// Size in bytes.
    pub size_bytes: i64,
    /// Display order within the file list.
    pub sort_order: i32,
}

impl AttachHomeworkFileCommand {
    /// Creates a command from already-uploaded file metadata.
    pub fn from_uploaded_file(
        homework_id: Uuid,
        storage_key: String,
        file_name: String,
        mime_type: String,
        size_bytes: i64,
        sort_order: i32,
    ) -> Self {
        Self {
            homework_id,
            file_id: None,
            storage_key,
            file_name,
            mime_type,
            size_bytes,
            sort_order,
        }
    }

    /// Validates the command and resolves the final `HomeworkFile` entity.
    pub fn to_homework_file(&self) -> Result<HomeworkFile, DomainError> {
        let file_id = self.file_id.unwrap_or_else(Uuid::new_v4);
        HomeworkFile::try_new(
            file_id,
            self.homework_id,
            self.storage_key.clone(),
            self.file_name.clone(),
            self.mime_type.clone(),
            self.size_bytes,
            self.sort_order,
        )
    }
}

/// Use case that associates an uploaded file with a homework.
pub struct AttachHomeworkFileUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
}

impl AttachHomeworkFileUseCase {
    /// Creates a new use case.
    pub fn new(homeworks: Arc<dyn HomeworkRepository>) -> Self {
        Self { homeworks }
    }

    /// Executes the attach flow.
    ///
    /// First validates the file metadata, then ensures the target homework exists,
    /// and finally persists the file in the repository.
    pub async fn execute(&self, cmd: AttachHomeworkFileCommand) -> Result<HomeworkFile, DomainError> {
        // Validate the metadata up front. This catches invalid storage keys,
        // empty names, oversized names, and negative file sizes before a DB write.
        let file = cmd.to_homework_file()?;

        // Ensure the owning homework exists. The repository will also reject
        // missing parent rows via FK violation mapping.
        self.homeworks.get_by_id(file.homework_id).await?;

        self.homeworks.add_file(file).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::homework::Homework;
    use domain::value_objects::homework_status::HomeworkStatus;
    use domain::value_objects::role::UserRole;

    struct MockHomeworkRepository {
        homework_exists: bool,
        was_added: std::sync::Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, _homework_id: Uuid) -> Result<Homework, DomainError> {
            if self.homework_exists {
                Ok(Homework::try_new(
                    _homework_id,
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    UserRole::Teacher,
                    None,
                    HomeworkStatus::Draft,
                    false,
                    None,
                    chrono::Utc::now(),
                )?)
            } else {
                Err(DomainError::HomeworkNotFound)
            }
        }

        async fn get_by_lesson_instance(
            &self,
            _lesson_instance_id: Uuid,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, _homework_id: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _homework: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, file: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            *self.was_added.lock().unwrap() = true;
            Ok(file)
        }

        async fn remove_file(&self, _file_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _homework_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_with_files(
            &self,
            _homework: Homework,
            _files: Vec<HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_attach_homework_file_success() {
        let repo = Arc::new(MockHomeworkRepository {
            homework_exists: true,
            was_added: std::sync::Mutex::new(false),
        });
        let use_case = AttachHomeworkFileUseCase::new(repo.clone());

        let cmd = AttachHomeworkFileCommand::from_uploaded_file(
            Uuid::new_v4(),
            "homeworks/sample.pdf".to_string(),
            "sample.pdf".to_string(),
            "application/pdf".to_string(),
            42,
            0,
        );

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        assert!(*repo.was_added.lock().unwrap());
    }

    #[tokio::test]
    async fn test_attach_homework_file_rejects_missing_homework() {
        let repo = Arc::new(MockHomeworkRepository {
            homework_exists: false,
            was_added: std::sync::Mutex::new(false),
        });
        let use_case = AttachHomeworkFileUseCase::new(repo);

        let cmd = AttachHomeworkFileCommand::from_uploaded_file(
            Uuid::new_v4(),
            "homeworks/sample.pdf".to_string(),
            "sample.pdf".to_string(),
            "application/pdf".to_string(),
            42,
            0,
        );

        let result = use_case.execute(cmd).await;
        assert_eq!(result, Err(DomainError::HomeworkNotFound));
    }
}
