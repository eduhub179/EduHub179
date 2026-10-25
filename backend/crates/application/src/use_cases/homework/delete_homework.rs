//! Use case: delete a homework with all its files.
//!
//! Flow:
//! 1. Read the file list (storage keys come from the DB).
//! 2. Delete the homework; file records go with it via `ON DELETE CASCADE`.
//! 3. Delete the objects from storage.
//!
//! Order: DB → Storage, as in `delete_file`: if the DB delete fails, storage is untouched.
//! Objects that could not be removed are returned as orphaned for logging.
//!
//! Dependencies: `domain` (HomeworkRepository, FileStorage).
//! Guarantees: returns `Result`, no panics.

use std::sync::Arc;

use domain::{
    errors::DomainError, ports::file_storage::FileStorage,
    repositories::homework_repository::HomeworkRepository,
};
use uuid::Uuid;

/// Result of a successful delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedHomework {
    /// Lesson the homework belonged to (to notify its students after the delete).
    pub lesson_instance_id: Uuid,
    /// Storage keys of files whose objects stayed in storage (storage failure or not configured).
    /// The application layer has no logger: the caller must log them for cleanup.
    pub orphaned_storage_keys: Vec<String>,
}

pub struct DeleteHomeworkUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    /// `None` if object storage is not configured: then file objects cannot be removed
    /// and are reported as orphaned (the homework itself is still deleted).
    storage: Option<Arc<dyn FileStorage>>,
}

impl DeleteHomeworkUseCase {
    pub fn new(
        homeworks: Arc<dyn HomeworkRepository>,
        storage: Option<Arc<dyn FileStorage>>,
    ) -> Self {
        Self { homeworks, storage }
    }

    /// # Fail-safe поведение
    /// `Err(HomeworkNotFound)` if there is no such homework; storage is not touched then.
    /// A storage failure after the DB delete does not fail the operation: for the user the
    /// homework is gone, the leftovers are listed in `orphaned_storage_keys`.
    pub async fn execute(&self, homework_id: Uuid) -> Result<DeletedHomework, DomainError> {
        let homework = self.homeworks.get_by_id(homework_id).await?;
        let files = self.homeworks.get_files(homework_id).await?;
        self.homeworks.delete(homework_id).await?;

        let mut orphaned_storage_keys = Vec::new();
        for file in files {
            let removed = match &self.storage {
                Some(storage) => storage.delete(&file.storage_key).await.is_ok(),
                None => false,
            };
            if !removed {
                orphaned_storage_keys.push(file.storage_key);
            }
        }

        Ok(DeletedHomework {
            lesson_instance_id: homework.lesson_instance_id,
            orphaned_storage_keys,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::homework::{Homework, HomeworkFile};
    use domain::ports::file_storage::{StoredFile, UploadFileRequest};
    use std::sync::Mutex;

    struct MockHomeworkRepository {
        exists: bool,
        files: Vec<HomeworkFile>,
        deleted: Mutex<bool>,
    }

    impl MockHomeworkRepository {
        fn with_files(keys: &[&str]) -> Self {
            let homework_id = Uuid::new_v4();
            Self {
                exists: true,
                files: keys
                    .iter()
                    .enumerate()
                    .map(|(index, key)| {
                        HomeworkFile::try_new(
                            Uuid::new_v4(),
                            homework_id,
                            key.to_string(),
                            "a.pdf".to_string(),
                            "application/pdf".to_string(),
                            1,
                            index as i32,
                        )
                        .unwrap()
                    })
                    .collect(),
                deleted: Mutex::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, homework_id: Uuid) -> Result<Homework, DomainError> {
            if !self.exists {
                return Err(DomainError::HomeworkNotFound);
            }
            Homework::try_new(
                homework_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                domain::value_objects::role::UserRole::Teacher,
                None,
                domain::value_objects::homework_status::HomeworkStatus::Published,
                false,
                None,
                chrono::Utc::now(),
            )
        }

        async fn get_by_lesson_instance(&self, _: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, _: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            // Like the real repository: a missing homework simply has no files.
            Ok(if self.exists { self.files.clone() } else { vec![] })
        }

        async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, _: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn remove_file(&self, _: Uuid) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _: Uuid) -> Result<(), DomainError> {
            if !self.exists {
                return Err(DomainError::HomeworkNotFound);
            }
            *self.deleted.lock().unwrap() = true;
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

    struct MockFileStorage {
        failing_key: Option<&'static str>,
        deleted: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, _: UploadFileRequest) -> Result<StoredFile, DomainError> {
            unimplemented!()
        }

        async fn download(&self, _: &str) -> Result<Vec<u8>, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
            if self.failing_key == Some(storage_key) {
                return Err(DomainError::InternalError);
            }
            self.deleted.lock().unwrap().push(storage_key.to_string());
            Ok(())
        }
    }

    fn storage(failing_key: Option<&'static str>) -> Arc<MockFileStorage> {
        Arc::new(MockFileStorage {
            failing_key,
            deleted: Mutex::new(vec![]),
        })
    }

    #[tokio::test]
    async fn deletes_homework_and_its_objects() {
        let repo = Arc::new(MockHomeworkRepository::with_files(&["homeworks/a", "homeworks/b"]));
        let storage = storage(None);
        let use_case = DeleteHomeworkUseCase::new(repo.clone(), Some(storage.clone()));

        let result = use_case.execute(Uuid::new_v4()).await.unwrap();

        assert!(*repo.deleted.lock().unwrap());
        assert!(result.orphaned_storage_keys.is_empty());
        assert_ne!(result.lesson_instance_id, Uuid::nil());
        assert_eq!(*storage.deleted.lock().unwrap(), vec!["homeworks/a", "homeworks/b"]);
    }

    #[tokio::test]
    async fn missing_homework_does_not_touch_storage() {
        let mut repo = MockHomeworkRepository::with_files(&["homeworks/a"]);
        repo.exists = false;
        let storage = storage(None);
        let use_case = DeleteHomeworkUseCase::new(Arc::new(repo), Some(storage.clone()));

        let result = use_case.execute(Uuid::new_v4()).await;

        assert_eq!(result, Err(DomainError::HomeworkNotFound));
        assert!(storage.deleted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn storage_failure_is_reported_not_returned() {
        let repo = Arc::new(MockHomeworkRepository::with_files(&["homeworks/a", "homeworks/b"]));
        let use_case =
            DeleteHomeworkUseCase::new(repo.clone(), Some(storage(Some("homeworks/a"))));

        let result = use_case.execute(Uuid::new_v4()).await.unwrap();

        assert!(*repo.deleted.lock().unwrap());
        assert_eq!(result.orphaned_storage_keys, vec!["homeworks/a"]);
    }

    #[tokio::test]
    async fn without_storage_all_objects_are_orphaned() {
        let repo = Arc::new(MockHomeworkRepository::with_files(&["homeworks/a"]));
        let use_case = DeleteHomeworkUseCase::new(repo.clone(), None);

        let result = use_case.execute(Uuid::new_v4()).await.unwrap();

        assert!(*repo.deleted.lock().unwrap());
        assert_eq!(result.orphaned_storage_keys, vec!["homeworks/a"]);
    }
}
