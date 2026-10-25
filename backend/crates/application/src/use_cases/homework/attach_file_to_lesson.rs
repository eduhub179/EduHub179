//! Use case: attach a file to a lesson (to the homework of a concrete lesson instance).
//!
//! Flow:
//! 1. Validate the file (`UploadHomeworkFileCommand::validate`).
//! 2. Upload the bytes to `FileStorage`.
//! 3. Save the metadata:
//!    - the lesson already has homework → `add_file` to it;
//!    - no homework yet → `create_with_files`: a file-only homework and the file in one
//!      transaction, so an empty homework (no text, no files) can never be left behind.
//! 4. If step 3 fails, the uploaded object is deleted (compensation), so storage
//!    does not collect objects without a DB record.
//!
//! Dependencies: `domain` (HomeworkRepository, FileStorage).
//! Guarantees: returns `Result`, no panics.

use std::sync::Arc;

use chrono::Utc;
use domain::{
    entities::homework::{Homework, HomeworkFile},
    errors::DomainError,
    ports::file_storage::{FileStorage, StoredFile, UploadFileRequest},
    repositories::homework_repository::HomeworkRepository,
    value_objects::{homework_status::HomeworkStatus, role::UserRole},
};
use uuid::Uuid;

use super::upload_file::UploadHomeworkFileCommand;

/// Input for attaching a file to a lesson.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachFileToLessonCommand {
    /// Concrete lesson on a concrete date.
    pub lesson_instance_id: Uuid,
    /// Who attaches the file; becomes the homework author if the homework is created now.
    pub uploaded_by: Uuid,
    pub uploaded_by_role: UserRole,
    pub file: UploadHomeworkFileCommand,
}

/// Result of a successful attach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedLessonFile {
    pub homework_id: Uuid,
    pub file: HomeworkFile,
    /// True if the lesson had no homework and it was created together with this file.
    pub homework_created: bool,
}

/// Uploads a file and attaches it to the homework of a lesson instance.
pub struct AttachFileToLessonUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    storage: Arc<dyn FileStorage>,
}

impl AttachFileToLessonUseCase {
    pub fn new(homeworks: Arc<dyn HomeworkRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { homeworks, storage }
    }

    /// # Fail-safe поведение
    /// - invalid file → `InvalidHomeworkFileFormat` / `InvalidHomeworkFileSize`, storage untouched;
    /// - unknown lesson → `LessonInstanceNotFound`, the uploaded object is deleted;
    /// - storage or DB failure → `Err`, nothing stays half-attached
    ///   (except an orphaned object if the compensating delete itself fails).
    pub async fn execute(
        &self,
        cmd: AttachFileToLessonCommand,
    ) -> Result<AttachedLessonFile, DomainError> {
        cmd.file.validate()?;

        let stored = self
            .storage
            .upload(UploadFileRequest {
                folder: domain::ports::file_storage::StorageFolder::Homeworks,
                file_name: cmd.file.file_name.trim().to_string(),
                mime_type: cmd.file.mime_type.trim().to_string(),
                bytes: cmd.file.bytes,
            })
            .await?;

        match self
            .save_metadata(cmd.lesson_instance_id, cmd.uploaded_by, cmd.uploaded_by_role, &stored)
            .await
        {
            Ok(attached) => Ok(attached),
            Err(error) => {
                // Best effort: a failed cleanup leaves an orphaned object, not a broken record.
                let _ = self.storage.delete(&stored.storage_key).await;
                Err(error)
            }
        }
    }

    async fn save_metadata(
        &self,
        lesson_instance_id: Uuid,
        uploaded_by: Uuid,
        uploaded_by_role: UserRole,
        stored: &StoredFile,
    ) -> Result<AttachedLessonFile, DomainError> {
        match self.homeworks.get_by_lesson_instance(lesson_instance_id).await {
            Ok(homework) => self.add_to_existing(homework.id, stored).await,
            Err(DomainError::HomeworkNotFound) => {
                let homework = Homework::try_new(
                    Uuid::new_v4(),
                    lesson_instance_id,
                    uploaded_by,
                    uploaded_by_role,
                    None,
                    HomeworkStatus::Published,
                    false,
                    None,
                    Utc::now(),
                )?;
                let file = Self::file_record(homework.id, stored, 0)?;

                match self.homeworks.create_with_files(homework, vec![file.clone()]).await {
                    Ok(created) => Ok(AttachedLessonFile {
                        homework_id: created.id,
                        file,
                        homework_created: true,
                    }),
                    // Someone created the homework between our read and insert: attach to theirs.
                    Err(DomainError::HomeworkAlreadyExists) => {
                        let homework =
                            self.homeworks.get_by_lesson_instance(lesson_instance_id).await?;
                        self.add_to_existing(homework.id, stored).await
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn add_to_existing(
        &self,
        homework_id: Uuid,
        stored: &StoredFile,
    ) -> Result<AttachedLessonFile, DomainError> {
        // New file goes to the end of the list.
        let sort_order = self
            .homeworks
            .get_files(homework_id)
            .await?
            .iter()
            .map(|file| file.sort_order)
            .max()
            .map_or(0, |last| last.saturating_add(1));
        let file = self
            .homeworks
            .add_file(Self::file_record(homework_id, stored, sort_order)?)
            .await?;

        Ok(AttachedLessonFile {
            homework_id,
            file,
            homework_created: false,
        })
    }

    fn file_record(
        homework_id: Uuid,
        stored: &StoredFile,
        sort_order: i32,
    ) -> Result<HomeworkFile, DomainError> {
        HomeworkFile::try_new(
            Uuid::new_v4(),
            homework_id,
            stored.storage_key.clone(),
            stored.file_name.clone(),
            stored.mime_type.clone(),
            stored.size_bytes,
            sort_order,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Homework repository in memory: at most one homework, its files.
    #[derive(Default)]
    struct MockHomeworkRepository {
        homework: Mutex<Option<Homework>>,
        files: Mutex<Vec<HomeworkFile>>,
        fail_add_file: bool,
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepository {
        async fn get_by_id(&self, _: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_by_lesson_instance(&self, _: Uuid) -> Result<Homework, DomainError> {
            self.homework
                .lock()
                .unwrap()
                .clone()
                .ok_or(DomainError::HomeworkNotFound)
        }

        async fn get_files(&self, _: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            Ok(self.files.lock().unwrap().clone())
        }

        async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, file: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            if self.fail_add_file {
                return Err(DomainError::InternalError);
            }
            self.files.lock().unwrap().push(file.clone());
            Ok(file)
        }

        async fn remove_file(&self, _: Uuid) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_with_files(
            &self,
            homework: Homework,
            files: Vec<HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            *self.homework.lock().unwrap() = Some(homework.clone());
            self.files.lock().unwrap().extend(files);
            Ok(homework)
        }
    }

    /// Records uploaded and deleted keys.
    #[derive(Default)]
    struct MockFileStorage {
        uploaded: Mutex<Vec<String>>,
        deleted: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError> {
            let key = format!("homeworks/{}_{}", Uuid::new_v4(), request.file_name);
            self.uploaded.lock().unwrap().push(key.clone());
            Ok(StoredFile {
                storage_key: key,
                file_name: request.file_name,
                mime_type: request.mime_type,
                size_bytes: request.bytes.len() as i64,
            })
        }

        async fn download(&self, _: &str) -> Result<Vec<u8>, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
            self.deleted.lock().unwrap().push(storage_key.to_string());
            Ok(())
        }
    }

    fn command(file_name: &str, bytes: Vec<u8>) -> AttachFileToLessonCommand {
        AttachFileToLessonCommand {
            lesson_instance_id: Uuid::new_v4(),
            uploaded_by: Uuid::new_v4(),
            uploaded_by_role: UserRole::Teacher,
            file: UploadHomeworkFileCommand::new(
                file_name.to_string(),
                "application/pdf".to_string(),
                bytes,
            ),
        }
    }

    #[tokio::test]
    async fn creates_homework_with_file_when_lesson_has_none() {
        let repo = Arc::new(MockHomeworkRepository::default());
        let use_case =
            AttachFileToLessonUseCase::new(repo.clone(), Arc::new(MockFileStorage::default()));

        let attached = use_case.execute(command("a.pdf", vec![1, 2])).await.unwrap();

        assert!(attached.homework_created);
        assert_eq!(attached.file.sort_order, 0);
        let homework = repo.homework.lock().unwrap().clone().unwrap();
        assert_eq!(homework.id, attached.homework_id);
        assert_eq!(homework.text_content, None);
        assert_eq!(repo.files.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn appends_file_to_existing_homework() {
        let repo = Arc::new(MockHomeworkRepository::default());
        let use_case =
            AttachFileToLessonUseCase::new(repo.clone(), Arc::new(MockFileStorage::default()));
        let first = use_case.execute(command("a.pdf", vec![1])).await.unwrap();

        let second = use_case.execute(command("b.pdf", vec![2])).await.unwrap();

        assert!(!second.homework_created);
        assert_eq!(second.homework_id, first.homework_id);
        assert_eq!(second.file.sort_order, 1);
    }

    #[tokio::test]
    async fn invalid_file_does_not_touch_storage() {
        let storage = Arc::new(MockFileStorage::default());
        let use_case = AttachFileToLessonUseCase::new(
            Arc::new(MockHomeworkRepository::default()),
            storage.clone(),
        );

        let result = use_case.execute(command("a.pdf", vec![])).await;

        assert_eq!(result, Err(DomainError::InvalidHomeworkFileSize));
        assert!(storage.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn db_failure_deletes_uploaded_object() {
        let repo = Arc::new(MockHomeworkRepository {
            fail_add_file: true,
            ..Default::default()
        });
        let storage = Arc::new(MockFileStorage::default());
        let use_case = AttachFileToLessonUseCase::new(repo.clone(), storage.clone());
        // The lesson already has homework, so the file goes through the failing `add_file`.
        *repo.homework.lock().unwrap() = Some(
            Homework::try_new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                UserRole::Teacher,
                Some("text".to_string()),
                HomeworkStatus::Published,
                false,
                None,
                Utc::now(),
            )
            .unwrap(),
        );

        let result = use_case.execute(command("a.pdf", vec![1])).await;

        assert_eq!(result, Err(DomainError::InternalError));
        assert_eq!(*storage.deleted.lock().unwrap(), *storage.uploaded.lock().unwrap());
    }
}
