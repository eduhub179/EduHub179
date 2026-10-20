//! Integration tests for `S3FileStorage` against a running MinIO (infra/docker-compose.yml).
//!
//! Ignored by default: they need MinIO, not just Postgres. Run from `backend/`:
//!     cargo test -p infrastructure --test s3_file_storage_tests -- --ignored
//! Connection comes from `S3_ENDPOINT` / `S3_ACCESS_KEY` / `S3_SECRET_KEY` in .env;
//! the tests use their own bucket `eduhub179-test`, never the working one.

use domain::errors::DomainError;
use domain::ports::file_storage::{FileStorage, UploadFileRequest};
use infrastructure::storage::{S3FileStorage, S3Settings};

const TEST_BUCKET: &str = "eduhub179-test";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

async fn storage() -> S3FileStorage {
    dotenvy::dotenv().ok();
    let storage = S3FileStorage::new(&S3Settings {
        endpoint: env_or("S3_ENDPOINT", "http://localhost:9000"),
        access_key: env_or("S3_ACCESS_KEY", "minioadmin"),
        secret_key: env_or("S3_SECRET_KEY", "minioadmin"),
        bucket: TEST_BUCKET.to_string(),
        region: env_or("S3_REGION", "us-east-1"),
    });
    storage
        .ensure_bucket()
        .await
        .expect("MinIO must be running (cd infra && docker compose up -d minio)");
    storage
}

fn pdf(bytes: Vec<u8>) -> UploadFileRequest {
    UploadFileRequest {
        file_name: "листок 1.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes,
    }
}

#[tokio::test]
#[ignore = "needs MinIO"]
async fn ensure_bucket_is_idempotent() {
    let storage = storage().await;

    assert_eq!(storage.ensure_bucket().await, Ok(()));
}

#[tokio::test]
#[ignore = "needs MinIO"]
async fn upload_download_delete_roundtrip() {
    let storage = storage().await;
    let bytes: Vec<u8> = (0..=255).cycle().take(300_000).collect();

    let stored = storage.upload(pdf(bytes.clone())).await.unwrap();

    assert!(stored.storage_key.starts_with("homeworks/"));
    assert!(stored.storage_key.ends_with("_листок_1.pdf"), "{}", stored.storage_key);
    assert_eq!(stored.size_bytes, 300_000);
    assert_eq!(storage.download(&stored.storage_key).await.unwrap(), bytes);

    assert_eq!(storage.delete(&stored.storage_key).await, Ok(()));
    assert_eq!(
        storage.download(&stored.storage_key).await,
        Err(DomainError::HomeworkFileNotFound)
    );
}

#[tokio::test]
#[ignore = "needs MinIO"]
async fn delete_missing_key_is_ok() {
    let storage = storage().await;

    assert_eq!(storage.delete("homeworks/does-not-exist.pdf").await, Ok(()));
}

#[tokio::test]
#[ignore = "needs MinIO"]
async fn download_missing_key_is_not_found() {
    let storage = storage().await;

    assert_eq!(
        storage.download("homeworks/does-not-exist.pdf").await,
        Err(DomainError::HomeworkFileNotFound)
    );
}

#[tokio::test]
#[ignore = "needs MinIO"]
async fn wrong_keys_give_internal_error() {
    let storage = S3FileStorage::new(&S3Settings {
        endpoint: env_or("S3_ENDPOINT", "http://localhost:9000"),
        access_key: "wrong".to_string(),
        secret_key: "wrong-secret".to_string(),
        bucket: TEST_BUCKET.to_string(),
        region: "us-east-1".to_string(),
    });

    assert_eq!(storage.ensure_bucket().await, Err(DomainError::InternalError));
}
