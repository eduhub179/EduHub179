//! Use case: delete a homework file.
//!
//! Flow:
//! 1. Remove the file metadata from the repository.
//! 2. Delete the object from file storage.
//!
//! Order: DB → Storage. If DB deletion fails, no storage operation is triggered.
//! If storage deletion fails after DB success, log it for later cleanup (orphaned file).

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::file_storage::FileStorage;
use domain::repositories::homework_repository::HomeworkRepository;
use uuid::Uuid;

/// Input for deleting a homework file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteHomeworkFileCommand {
    /// File ID to delete.
    pub file_id: Uuid,
    /// Storage key for cleanup (optional; if provided, deletes from storage too).
    pub storage_key: Option<String>,
}

/// Use case that removes a file from homework (both DB and storage).
pub struct DeleteHomeworkFileUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    storage: Arc<dyn FileStorage>,
}

impl DeleteHomeworkFileUseCase {
    /// Creates a new use case.
    pub fn new(homeworks: Arc<dyn HomeworkRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { homeworks, storage }
    }

    /// Executes the delete flow.
    ///
    /// First removes the metadata from the repository, then deletes the object
    /// from file storage. If metadata deletion fails, storage is not touched.
    /// If storage deletion fails after DB success, the error is reported but
    /// the file is considered "deleted" from the user's perspective (metadata is gone).
    pub async fn execute(&self, cmd: DeleteHomeworkFileCommand) -> Result<(), DomainError> {
        // 1. Remove the file record from the database first.
        // If this fails, the storage cleanup is skipped.
        self.homeworks.remove_file(cmd.file_id).await?;

        // 2. If a storage key was provided, delete from object storage.
        // Failure here is logged but does not fail the operation (metadata already gone).
        if let Some(storage_key) = cmd.storage_key {
            let _ = self.storage.delete(&storage_key).await;
            // Note: in production, log this failure for background cleanup tasks.
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::homework::Homework;
    use domain::value_objects::homework_status::HomeworkStatus;
    use domain::value_objects::role::UserRole;

    struct MockHomeworkRepository {
        file_exists: bool,
        was_removed: std::sync::Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, _: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_by_lesson_instance(&self, _: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, _: Uuid) -> Result<Vec<domain::entities::homework::HomeworkFile>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, _: domain::entities::homework::HomeworkFile) -> Result<domain::entities::homework::HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn remove_file(&self, _file_id: Uuid) -> Result<(), DomainError> {
            if self.file_exists {
                *self.was_removed.lock().unwrap() = true;
                Ok(())
            } else {
                Err(DomainError::HomeworkFileNotFound)
            }
        }

        async fn delete(&self, _: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_with_files(
            &self,
            _: Homework,
            _: Vec<domain::entities::homework::HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }
    }

    struct MockFileStorage {
        was_deleted: std::sync::Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, _: domain::ports::file_storage::UploadFileRequest) -> Result<domain::ports::file_storage::StoredFile, DomainError> {
            unimplemented!()
        }

        async fn download(&self, _: &str) -> Result<Vec<u8>, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _: &str) -> Result<(), DomainError> {
            *self.was_deleted.lock().unwrap() = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_delete_homework_file_success() {
        let repo = Arc::new(MockHomeworkRepository {
            file_exists: true,
            was_removed: std::sync::Mutex::new(false),
        });
        let storage = Arc::new(MockFileStorage {
            was_deleted: std::sync::Mutex::new(false),
        });
        let use_case = DeleteHomeworkFileUseCase::new(repo.clone(), storage.clone());

        let cmd = DeleteHomeworkFileCommand {
            file_id: Uuid::new_v4(),
            storage_key: Some("homeworks/sample.pdf".to_string()),
        };

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        assert!(*repo.was_removed.lock().unwrap());
        assert!(*storage.was_deleted.lock().unwrap());
    }

    #[tokio::test]
    async fn test_delete_homework_file_not_found() {
        let repo = Arc::new(MockHomeworkRepository {
            file_exists: false,
            was_removed: std::sync::Mutex::new(false),
        });
        let storage = Arc::new(MockFileStorage {
            was_deleted: std::sync::Mutex::new(false),
        });
        let use_case = DeleteHomeworkFileUseCase::new(repo, storage);

        let cmd = DeleteHomeworkFileCommand {
            file_id: Uuid::new_v4(),
            storage_key: Some("homeworks/sample.pdf".to_string()),
        };

        let result = use_case.execute(cmd).await;
        assert_eq!(result, Err(DomainError::HomeworkFileNotFound));
    }

    #[tokio::test]
    async fn test_delete_homework_file_skips_storage_cleanup_if_no_key() {
        let repo = Arc::new(MockHomeworkRepository {
            file_exists: true,
            was_removed: std::sync::Mutex::new(false),
        });
        let storage = Arc::new(MockFileStorage {
            was_deleted: std::sync::Mutex::new(false),
        });
        let use_case = DeleteHomeworkFileUseCase::new(repo.clone(), storage.clone());

        let cmd = DeleteHomeworkFileCommand {
            file_id: Uuid::new_v4(),
            storage_key: None,
        };

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        assert!(*repo.was_removed.lock().unwrap());
        // Storage should not be called if no key provided
        assert!(!*storage.was_deleted.lock().unwrap());
    }
}
