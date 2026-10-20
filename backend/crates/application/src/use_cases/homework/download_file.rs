//! Use case: download a homework file.
//!
//! The file is looked up within its homework (`homework_id` + `file_id`), and the bytes are
//! read by the `storage_key` from the DB — the client never passes storage keys.
//!
//! Dependencies: `domain` (HomeworkRepository, FileStorage).
//! Guarantees: returns `Result`, no panics.

use std::sync::Arc;

use domain::{
    entities::homework::HomeworkFile, errors::DomainError, ports::file_storage::FileStorage,
    repositories::homework_repository::HomeworkRepository,
};
use uuid::Uuid;

/// File metadata (name, MIME type for the response headers) and its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedHomeworkFile {
    pub file: HomeworkFile,
    pub bytes: Vec<u8>,
}

pub struct DownloadHomeworkFileUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    storage: Arc<dyn FileStorage>,
}

impl DownloadHomeworkFileUseCase {
    pub fn new(homeworks: Arc<dyn HomeworkRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { homeworks, storage }
    }

    /// # Fail-safe поведение
    /// `HomeworkFileNotFound` if the file does not belong to this homework or its object
    /// is missing in storage; `InternalError` on storage failure.
    pub async fn execute(
        &self,
        homework_id: Uuid,
        file_id: Uuid,
    ) -> Result<DownloadedHomeworkFile, DomainError> {
        let file = self
            .homeworks
            .get_files(homework_id)
            .await?
            .into_iter()
            .find(|file| file.id == file_id)
            .ok_or(DomainError::HomeworkFileNotFound)?;
        let bytes = self.storage.download(&file.storage_key).await?;

        Ok(DownloadedHomeworkFile { file, bytes })
    }
}
