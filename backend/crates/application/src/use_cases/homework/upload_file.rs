//! Use case: upload a homework file.
//!
//! Flow:
//! 1. Validate input: file name, MIME type, size constraints.
//! 2. Call `FileStorage::upload` to persist raw bytes.
//! 3. Return `StoredFile` metadata for database insertion.
//!
//! Constraints:
//! - File name must be non-empty (trimmed).
//! - MIME type must be non-empty (trimmed).
//! - File size must be > 0 and <= 100 MB by default.

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::file_storage::{FileStorage, UploadFileRequest, StoredFile};

/// Maximum file size: 100 MB.
const MAX_FILE_SIZE: i64 = 100 * 1024 * 1024;

/// Input for uploading a homework file to object storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadHomeworkFileCommand {
    /// Original file name as sent by the client (will be trimmed).
    pub file_name: String,
    /// MIME type reported by the client or detected by the app (will be trimmed).
    pub mime_type: String,
    /// Raw file contents.
    pub bytes: Vec<u8>,
}

impl UploadHomeworkFileCommand {
    /// Creates a command from client input.
    pub fn new(file_name: String, mime_type: String, bytes: Vec<u8>) -> Self {
        Self {
            file_name,
            mime_type,
            bytes,
        }
    }

    /// Validates the command before uploading.
    ///
    /// Returns `Err` if:
    /// - `file_name` or `mime_type` are empty after trimming.
    /// - File size is 0 or exceeds the limit.
    pub fn validate(&self) -> Result<(), DomainError> {
        let name = self.file_name.trim();
        let mime = self.mime_type.trim();

        if name.is_empty() || mime.is_empty() {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        let size = i64::try_from(self.bytes.len()).unwrap_or(i64::MAX);
        if size == 0 || size > MAX_FILE_SIZE {
            return Err(DomainError::InvalidHomeworkFileSize);
        }

        Ok(())
    }
}

/// Use case that uploads a file to object storage.
pub struct UploadHomeworkFileUseCase {
    storage: Arc<dyn FileStorage>,
}

impl UploadHomeworkFileUseCase {
    /// Creates a new use case.
    pub fn new(storage: Arc<dyn FileStorage>) -> Self {
        Self { storage }
    }

    /// Executes the upload flow.
    ///
    /// Validates the command, then uploads the raw bytes to storage,
    /// and returns the metadata (storage key + original file name + MIME type + size).
    pub async fn execute(&self, cmd: UploadHomeworkFileCommand) -> Result<StoredFile, DomainError> {
        cmd.validate()?;

        let request = UploadFileRequest {
            file_name: cmd.file_name.trim().to_string(),
            mime_type: cmd.mime_type.trim().to_string(),
            bytes: cmd.bytes,
        };

        self.storage.upload(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFileStorage {
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError> {
            if self.should_fail {
                return Err(DomainError::InternalError);
            }

            Ok(StoredFile {
                storage_key: format!("homeworks/{}", uuid::Uuid::new_v4()),
                file_name: request.file_name,
                mime_type: request.mime_type,
                size_bytes: request.bytes.len() as i64,
            })
        }

        async fn download(&self, _: &str) -> Result<Vec<u8>, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _: &str) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    #[test]
    fn test_validate_rejects_empty_file_name() {
        let cmd = UploadHomeworkFileCommand::new(
            "".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3],
        );

        let result = cmd.validate();
        assert_eq!(result, Err(DomainError::InvalidHomeworkFileFormat));
    }

    #[test]
    fn test_validate_rejects_empty_mime_type() {
        let cmd = UploadHomeworkFileCommand::new(
            "file.pdf".to_string(),
            "".to_string(),
            vec![1, 2, 3],
        );

        let result = cmd.validate();
        assert_eq!(result, Err(DomainError::InvalidHomeworkFileFormat));
    }

    #[test]
    fn test_validate_rejects_empty_file() {
        let cmd = UploadHomeworkFileCommand::new(
            "file.pdf".to_string(),
            "application/pdf".to_string(),
            vec![],
        );

        let result = cmd.validate();
        assert_eq!(result, Err(DomainError::InvalidHomeworkFileSize));
    }

    #[test]
    fn test_validate_accepts_valid_input() {
        let cmd = UploadHomeworkFileCommand::new(
            "file.pdf".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3, 4, 5],
        );

        let result = cmd.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_trims_whitespace() {
        let cmd = UploadHomeworkFileCommand::new(
            "  file.pdf  ".to_string(),
            "  application/pdf  ".to_string(),
            vec![1, 2, 3],
        );

        let result = cmd.validate();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_upload_success() {
        let storage = Arc::new(MockFileStorage { should_fail: false });
        let use_case = UploadHomeworkFileUseCase::new(storage);

        let cmd = UploadHomeworkFileCommand::new(
            "sample.pdf".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3, 4, 5],
        );

        let result = use_case.execute(cmd).await;
        assert!(result.is_ok());
        let stored = result.unwrap();
        assert_eq!(stored.file_name, "sample.pdf");
        assert_eq!(stored.mime_type, "application/pdf");
        assert_eq!(stored.size_bytes, 5);
    }

    #[tokio::test]
    async fn test_upload_rejects_invalid_input() {
        let storage = Arc::new(MockFileStorage { should_fail: false });
        let use_case = UploadHomeworkFileUseCase::new(storage);

        let cmd = UploadHomeworkFileCommand::new(
            "".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3],
        );

        let result = use_case.execute(cmd).await;
        assert_eq!(result, Err(DomainError::InvalidHomeworkFileFormat));
    }

    #[tokio::test]
    async fn test_upload_handles_storage_failure() {
        let storage = Arc::new(MockFileStorage { should_fail: true });
        let use_case = UploadHomeworkFileUseCase::new(storage);

        let cmd = UploadHomeworkFileCommand::new(
            "sample.pdf".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3],
        );

        let result = use_case.execute(cmd).await;
        assert_eq!(result, Err(DomainError::InternalError));
    }
}
