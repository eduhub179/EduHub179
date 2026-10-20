//! In-memory file storage: the `FileStorage` port without a real object store.
//!
//! For tests and for running the backend without MinIO. Files live in process memory
//! and are lost on restart, so it must not be used in production (see `S3FileStorage`).
//! Behaves like S3 where it matters: same key format, idempotent `delete`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::errors::DomainError;
use domain::ports::file_storage::{FileStorage, StoredFile, UploadFileRequest};

use super::object_key;

/// In-memory object store. `Clone` shares the same storage (`Arc`).
#[derive(Debug, Clone, Default)]
pub struct InMemoryFileStorage {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryFileStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl FileStorage for InMemoryFileStorage {
    async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError> {
        let file_name = request.file_name.trim();
        let mime_type = request.mime_type.trim();

        if file_name.is_empty() || mime_type.is_empty() {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        let storage_key = object_key(file_name);
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

    /// Idempotent like S3 `DeleteObject`: a missing key is `Ok(())`, not an error.
    async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
        self.objects
            .lock()
            .map_err(|_| DomainError::InternalError)?
            .remove(storage_key);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(file_name: &str) -> UploadFileRequest {
        UploadFileRequest {
            file_name: file_name.to_string(),
            mime_type: "application/pdf".to_string(),
            bytes: vec![1, 2, 3],
        }
    }

    #[tokio::test]
    async fn key_has_homeworks_prefix() {
        let storage = InMemoryFileStorage::new();

        let stored = storage.upload(request("my sheet/1.pdf")).await.unwrap();

        assert!(stored.storage_key.starts_with("homeworks/"), "{}", stored.storage_key);
        assert!(stored.storage_key.ends_with("_my_sheet_1.pdf"), "{}", stored.storage_key);
        assert_eq!(stored.size_bytes, 3);
    }

    #[tokio::test]
    async fn same_name_gets_different_keys() {
        let storage = InMemoryFileStorage::new();

        let first = storage.upload(request("a.pdf")).await.unwrap();
        let second = storage.upload(request("a.pdf")).await.unwrap();

        assert_ne!(first.storage_key, second.storage_key);
    }

    #[tokio::test]
    async fn max_name_fits_db_key_limit() {
        let storage = InMemoryFileStorage::new();

        let stored = storage.upload(request(&"я".repeat(255))).await.unwrap();

        assert!(stored.storage_key.chars().count() <= 500);
    }

    #[tokio::test]
    async fn download_returns_uploaded_bytes() {
        let storage = InMemoryFileStorage::new();
        let stored = storage.upload(request("a.pdf")).await.unwrap();

        assert_eq!(storage.download(&stored.storage_key).await.unwrap(), vec![1, 2, 3]);
        assert_eq!(
            storage.download("homeworks/missing").await,
            Err(DomainError::HomeworkFileNotFound)
        );
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let storage = InMemoryFileStorage::new();
        let stored = storage.upload(request("a.pdf")).await.unwrap();

        assert_eq!(storage.delete(&stored.storage_key).await, Ok(()));
        assert_eq!(storage.delete(&stored.storage_key).await, Ok(()));
        assert_eq!(
            storage.download(&stored.storage_key).await,
            Err(DomainError::HomeworkFileNotFound)
        );
    }
}
