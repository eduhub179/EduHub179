//! File-storage ports: external object storage for lesson/homework attachments.
//!
//! Implemented by the `infrastructure` crate (`S3FileStorage`).
//! Consumed by application use cases for uploading/downloading homework files.
//!
//! This is intentionally separate from `HomeworkRepository` because the actual
//! file blobs live in object storage, while PostgreSQL stores only metadata.

use crate::errors::DomainError;

/// Data required to upload a file to object storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadFileRequest {
    /// Original file name as sent by the client.
    pub file_name: String,
    /// MIME type reported by the client or detected by the app.
    pub mime_type: String,
    /// Raw file contents.
    pub bytes: Vec<u8>,
}

/// Metadata returned after a successful upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    /// Unique storage key used by the object store (for example, a path or S3 object key).
    pub storage_key: String,
    /// Original file name to display to the user.
    pub file_name: String,
    /// MIME type of the stored file.
    pub mime_type: String,
    /// Size in bytes.
    pub size_bytes: i64,
}

/// Object-storage port used by homework/lesson attachment workflows.
#[async_trait::async_trait]
pub trait FileStorage: Send + Sync {
    /// Uploads the file content to the backing object store.
    ///
    /// The implementation decides the final key, but the result must include a
    /// stable `storage_key` that can later be used for downloads and deletes.
    async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError>;

    /// Downloads the raw bytes for the given storage key.
    async fn download(&self, storage_key: &str) -> Result<Vec<u8>, DomainError>;

    /// Deletes the object with the given storage key.
    async fn delete(&self, storage_key: &str) -> Result<(), DomainError>;
}
