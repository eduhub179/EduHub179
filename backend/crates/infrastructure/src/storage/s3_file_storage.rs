//! S3-compatible file-storage adapter.
//!
//! This is the infrastructure implementation of the domain `FileStorage` port.
//! It is intentionally thin: the production implementation may later swap the
//! in-memory stub for an AWS SDK or MinIO client, but the domain contract stays
//! unchanged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::errors::DomainError;
use domain::ports::file_storage::{FileStorage, StoredFile, UploadFileRequest};

/// In-memory S3-compatible adapter for bootstrapping and tests.
///
/// Production code should replace this implementation with a real object-store
/// client (AWS SDK / MinIO / Yandex Object Storage) without changing the public
/// contract used by application services.
#[derive(Debug, Clone, Default)]
pub struct S3FileStorage {
    bucket: String,
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl S3FileStorage {
    /// Creates an adapter bound to a bucket name.
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            objects: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the bucket name for the configured object store.
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    fn normalize_key(file_name: &str) -> String {
        let safe_name = file_name
            .trim()
            .replace(['/', '\\'], "_")
            .replace(' ', "_");

        format!("{}/{}", "homeworks", safe_name)
    }
}

#[async_trait::async_trait]
impl FileStorage for S3FileStorage {
    async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError> {
        let file_name = request.file_name.trim();
        let mime_type = request.mime_type.trim();

        if file_name.is_empty() || mime_type.is_empty() {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        let storage_key = format!(
            "{}/{}_{}",
            self.bucket,
            uuid::Uuid::new_v4(),
            Self::normalize_key(file_name)
        );
        let size_bytes = i64::try_from(request.bytes.len()).unwrap_or(i64::MAX);

        self.objects
            .lock()
            .map_err(|_| DomainError::InternalError)?
            .insert(storage_key.clone(), request.bytes);

        Ok(StoredFile {
            storage_key,
            file_name: file_name.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes,
        })
    }

    async fn download(&self, storage_key: &str) -> Result<Vec<u8>, DomainError> {
        let value = self
            .objects
            .lock()
            .map_err(|_| DomainError::InternalError)?
            .get(storage_key)
            .cloned();

        value.ok_or(DomainError::HomeworkFileNotFound)
    }

    async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
        let removed = self
            .objects
            .lock()
            .map_err(|_| DomainError::InternalError)?
            .remove(storage_key)
            .is_some();

        if removed {
            Ok(())
        } else {
            Err(DomainError::HomeworkFileNotFound)
        }
    }
}
