//! Storage adapters.
//!
//! This module contains external service adapters such as S3-compatible object
//! storage. The domain layer speaks only to the abstract `FileStorage` port.
//! The concrete implementations live here:
//! - `S3FileStorage` — real object store (MinIO locally and on our server, Yandex Object Storage);
//! - `InMemoryFileStorage` — for tests and running without MinIO.

pub mod in_memory_file_storage;
pub mod s3_file_storage;

pub use in_memory_file_storage::InMemoryFileStorage;
pub use s3_file_storage::{S3FileStorage, S3Settings};

/// Object key: `homeworks/<uuid>_<safe_name>`. Shared by all adapters, so switching
/// storage does not change the key format kept in `homework_files.storage_key`.
///
/// The bucket is not part of the key: S3 takes it as a separate parameter.
/// Separators and spaces are replaced 1:1, so the key stays within the 500-char DB limit
/// for any name accepted by the upload use case (≤ 255 chars).
pub(crate) fn object_key(file_name: &str) -> String {
    let safe_name = file_name.trim().replace(['/', '\\', ' '], "_");

    format!("homeworks/{}_{}", uuid::Uuid::new_v4(), safe_name)
}
