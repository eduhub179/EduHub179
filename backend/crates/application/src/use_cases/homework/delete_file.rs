//! Use case: delete a homework file.
//!
//! Flow:
//! 1. Remove the file record from the repository; it returns the deleted record.
//! 2. Delete the object from file storage by the `storage_key` of that record.
//!
//! The storage key always comes from the DB, never from the client: otherwise a caller
//! could delete their own file record and pass the key of someone else's object.
//!
//! Order: DB → Storage. If DB deletion fails, no storage operation is triggered.
//! If storage deletion fails after DB success, the key is returned as orphaned
//! so the caller can log it for later cleanup.
//!
//! A homework must have text or at least one file: if the removed file was the last
//! content of a homework without text, the homework is deleted too.

use std::sync::Arc;

use domain::entities::homework::HomeworkFile;
use domain::errors::DomainError;
use domain::ports::file_storage::FileStorage;
use domain::repositories::homework_repository::HomeworkRepository;
use uuid::Uuid;

/// Input for deleting a homework file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteHomeworkFileCommand {
    /// File ID to delete.
    pub file_id: Uuid,
}

/// Result of a successful delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedHomeworkFile {
    /// The removed metadata record.
    pub file: HomeworkFile,
    /// `Some(key)` if the record is gone but the object could not be removed from storage.
    /// The application layer has no logger: the caller must log it for background cleanup.
    pub orphaned_storage_key: Option<String>,
    /// True if the homework had no text and this was its last file, so it was deleted too.
    pub homework_deleted: bool,
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
    /// # Fail-safe поведение
    /// Returns `Err` only if the DB record was not removed (e.g. `HomeworkFileNotFound`);
    /// storage is not touched then. A storage failure after the DB delete does not fail
    /// the operation (for the user the file is gone): it is reported in `orphaned_storage_key`.
    /// If the empty homework cannot be deleted afterwards, the file is still gone and
    /// `homework_deleted` is false (the homework stays, visible as file-less).
    pub async fn execute(
        &self,
        cmd: DeleteHomeworkFileCommand,
    ) -> Result<DeletedHomeworkFile, DomainError> {
        let file = self.homeworks.remove_file(cmd.file_id).await?;

        let orphaned_storage_key = match self.storage.delete(&file.storage_key).await {
            Ok(()) => None,
            Err(_) => Some(file.storage_key.clone()),
        };
        let homework_deleted = self.delete_homework_if_empty(file.homework_id).await;

        Ok(DeletedHomeworkFile {
            file,
            orphaned_storage_key,
            homework_deleted,
        })
    }

    /// Best effort: any error leaves the homework in place and returns false.
    async fn delete_homework_if_empty(&self, homework_id: Uuid) -> bool {
        let has_text = match self.homeworks.get_by_id(homework_id).await {
            Ok(homework) => homework.text_content.is_some(),
            Err(_) => return false,
        };
        if has_text {
            return false;
        }
        match self.homeworks.get_files(homework_id).await {
            Ok(files) if files.is_empty() => self.homeworks.delete(homework_id).await.is_ok(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::homework::Homework;
    use std::sync::Mutex;

    const STORED_KEY: &str = "homeworks/stored-key_sample.pdf";

    struct MockHomeworkRepository {
        file_exists: bool,
        homework_text: Option<String>,
        remaining_files: usize,
        homework_deleted: Mutex<bool>,
    }

    impl MockHomeworkRepository {
        fn new(file_exists: bool) -> Self {
            Self {
                file_exists,
                homework_text: Some("text".to_string()),
                remaining_files: 0,
                homework_deleted: Mutex::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, homework_id: Uuid) -> Result<Homework, DomainError> {
            Homework::try_new(
                homework_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                domain::value_objects::role::UserRole::Teacher,
                self.homework_text.clone(),
                domain::value_objects::homework_status::HomeworkStatus::Published,
                false,
                None,
                chrono::Utc::now(),
            )
        }

        async fn get_by_lesson_instance(&self, _: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, homework_id: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            Ok((0..self.remaining_files)
                .map(|index| {
                    HomeworkFile::try_new(
                        Uuid::new_v4(),
                        homework_id,
                        format!("homeworks/other-{index}"),
                        "b.pdf".to_string(),
                        "application/pdf".to_string(),
                        1,
                        index as i32,
                    )
                    .unwrap()
                })
                .collect())
        }

        async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, _: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn remove_file(&self, file_id: Uuid) -> Result<HomeworkFile, DomainError> {
            if !self.file_exists {
                return Err(DomainError::HomeworkFileNotFound);
            }
            HomeworkFile::try_new(
                file_id,
                Uuid::new_v4(),
                STORED_KEY.to_string(),
                "sample.pdf".to_string(),
                "application/pdf".to_string(),
                42,
                0,
            )
        }

        async fn delete(&self, _: Uuid) -> Result<(), DomainError> {
            *self.homework_deleted.lock().unwrap() = true;
            Ok(())
        }

        async fn create_with_files(
            &self,
            _: Homework,
            _: Vec<HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }
    }

    /// Records which keys were deleted; can be told to fail.
    struct MockFileStorage {
        fail: bool,
        deleted: Mutex<Vec<String>>,
    }

    impl MockFileStorage {
        fn new(fail: bool) -> Self {
            Self {
                fail,
                deleted: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait::async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(
            &self,
            _: domain::ports::file_storage::UploadFileRequest,
        ) -> Result<domain::ports::file_storage::StoredFile, DomainError> {
            unimplemented!()
        }

        async fn download(&self, _: &str) -> Result<Vec<u8>, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
            if self.fail {
                return Err(DomainError::InternalError);
            }
            self.deleted.lock().unwrap().push(storage_key.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_delete_uses_storage_key_from_db() {
        let storage = Arc::new(MockFileStorage::new(false));
        let use_case = DeleteHomeworkFileUseCase::new(
            Arc::new(MockHomeworkRepository::new(true)),
            storage.clone(),
        );

        let result = use_case
            .execute(DeleteHomeworkFileCommand {
                file_id: Uuid::new_v4(),
            })
            .await
            .unwrap();

        assert_eq!(result.orphaned_storage_key, None);
        assert!(!result.homework_deleted, "homework with text must stay");
        assert_eq!(*storage.deleted.lock().unwrap(), vec![STORED_KEY.to_string()]);
    }

    #[tokio::test]
    async fn test_last_file_of_textless_homework_deletes_homework() {
        let repo = Arc::new(MockHomeworkRepository {
            homework_text: None,
            ..MockHomeworkRepository::new(true)
        });
        let use_case =
            DeleteHomeworkFileUseCase::new(repo.clone(), Arc::new(MockFileStorage::new(false)));

        let result = use_case
            .execute(DeleteHomeworkFileCommand {
                file_id: Uuid::new_v4(),
            })
            .await
            .unwrap();

        assert!(result.homework_deleted);
        assert!(*repo.homework_deleted.lock().unwrap());
    }

    #[tokio::test]
    async fn test_textless_homework_with_other_files_stays() {
        let repo = Arc::new(MockHomeworkRepository {
            homework_text: None,
            remaining_files: 1,
            ..MockHomeworkRepository::new(true)
        });
        let use_case =
            DeleteHomeworkFileUseCase::new(repo.clone(), Arc::new(MockFileStorage::new(false)));

        let result = use_case
            .execute(DeleteHomeworkFileCommand {
                file_id: Uuid::new_v4(),
            })
            .await
            .unwrap();

        assert!(!result.homework_deleted);
        assert!(!*repo.homework_deleted.lock().unwrap());
    }

    #[tokio::test]
    async fn test_delete_homework_file_not_found_skips_storage() {
        let storage = Arc::new(MockFileStorage::new(false));
        let use_case = DeleteHomeworkFileUseCase::new(
            Arc::new(MockHomeworkRepository::new(false)),
            storage.clone(),
        );

        let result = use_case
            .execute(DeleteHomeworkFileCommand {
                file_id: Uuid::new_v4(),
            })
            .await;

        assert_eq!(result, Err(DomainError::HomeworkFileNotFound));
        assert!(storage.deleted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_storage_failure_reports_orphaned_key() {
        let use_case = DeleteHomeworkFileUseCase::new(
            Arc::new(MockHomeworkRepository::new(true)),
            Arc::new(MockFileStorage::new(true)),
        );

        let result = use_case
            .execute(DeleteHomeworkFileCommand {
                file_id: Uuid::new_v4(),
            })
            .await
            .unwrap();

        assert_eq!(result.orphaned_storage_key, Some(STORED_KEY.to_string()));
    }
}
