//! S3-compatible file storage: implementation of the domain `FileStorage` port.
//!
//! Works with any S3 API: MinIO (locally from infra/docker-compose.yml and on our server)
//! or Yandex Object Storage — only `S3_ENDPOINT` and the keys in `.env` differ.
//!
//! Dependencies: `aws-sdk-s3`.
//! Guarantees: no panics; SDK errors are logged via `tracing` and mapped to `DomainError`
//! (a missing object → `HomeworkFileNotFound`, everything else → `InternalError`).

use aws_sdk_s3::{
    config::{BehaviorVersion, Credentials, Region},
    error::DisplayErrorContext,
    primitives::ByteStream,
    Client,
};
use domain::errors::DomainError;
use domain::ports::file_storage::{FileStorage, StoredFile, UploadFileRequest};

use super::object_key;

/// Connection settings (`S3_*` in backend/.env).
#[derive(Debug, Clone)]
pub struct S3Settings {
    /// For example `http://localhost:9000` (MinIO) or `https://storage.yandexcloud.net`.
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    /// MinIO ignores the region, but the request signature needs one (`us-east-1` by default).
    pub region: String,
}

/// Object store client bound to one bucket.
#[derive(Debug, Clone)]
pub struct S3FileStorage {
    client: Client,
    bucket: String,
}

impl S3FileStorage {
    /// Builds the client. Does not touch the network: a wrong endpoint or keys
    /// surface on the first request (see `ensure_bucket`).
    pub fn new(settings: &S3Settings) -> Self {
        let credentials = Credentials::new(
            &settings.access_key,
            &settings.secret_key,
            None,
            None,
            "eduhub-config",
        );
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&settings.endpoint)
            .region(Region::new(settings.region.clone()))
            .credentials_provider(credentials)
            // MinIO serves buckets as `host/bucket/key`, not as `bucket.host/key`.
            .force_path_style(true)
            .build();

        Self {
            client: Client::from_conf(config),
            bucket: settings.bucket.clone(),
        }
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Creates the bucket if it does not exist yet. Called once at startup.
    ///
    /// # Fail-safe поведение
    /// `Ok(())` if the bucket exists or was created (also when created concurrently).
    /// `Err(InternalError)` if the store is unreachable or the keys are wrong.
    pub async fn ensure_bucket(&self) -> Result<(), DomainError> {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => return Ok(()),
            Err(error) if error.as_service_error().is_some_and(|e| e.is_not_found()) => {}
            Err(error) => return Err(log_error("head_bucket", &self.bucket, error)),
        }

        match self.client.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(()),
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|e| e.is_bucket_already_owned_by_you()) =>
            {
                Ok(())
            }
            Err(error) => Err(log_error("create_bucket", &self.bucket, error)),
        }
    }
}

/// Logs the full SDK error (it never reaches the user) and hides it behind `InternalError`.
fn log_error<E: std::error::Error>(operation: &str, target: &str, error: E) -> DomainError {
    tracing::error!("s3 {operation} {target}: {}", DisplayErrorContext(&error));
    DomainError::InternalError
}

#[async_trait::async_trait]
impl FileStorage for S3FileStorage {
    async fn upload(&self, request: UploadFileRequest) -> Result<StoredFile, DomainError> {
        let file_name = request.file_name.trim();
        let mime_type = request.mime_type.trim();

        if file_name.is_empty() || mime_type.is_empty() {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }

        let storage_key = object_key(request.folder, file_name);
        let size_bytes = i64::try_from(request.bytes.len()).unwrap_or(i64::MAX);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&storage_key)
            .content_type(mime_type)
            .body(ByteStream::from(request.bytes))
            .send()
            .await
            .map_err(|error| log_error("put_object", &storage_key, error))?;

        Ok(StoredFile {
            storage_key,
            file_name: file_name.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes,
        })
    }

    async fn download(&self, storage_key: &str) -> Result<Vec<u8>, DomainError> {
        let output = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(storage_key)
            .send()
            .await
        {
            Ok(output) => output,
            Err(error) if error.as_service_error().is_some_and(|e| e.is_no_such_key()) => {
                return Err(DomainError::HomeworkFileNotFound)
            }
            Err(error) => return Err(log_error("get_object", storage_key, error)),
        };

        let body = output
            .body
            .collect()
            .await
            .map_err(|error| log_error("get_object body", storage_key, error))?;

        Ok(body.into_bytes().to_vec())
    }

    /// S3 `DeleteObject` is idempotent: a missing key is `Ok(())`.
    async fn delete(&self, storage_key: &str) -> Result<(), DomainError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(storage_key)
            .send()
            .await
            .map_err(|error| log_error("delete_object", storage_key, error))?;

        Ok(())
    }
}
