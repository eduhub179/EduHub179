//! Storage adapters.
//!
//! This module contains external service adapters such as S3-compatible object
//! storage. The domain layer speaks only to the abstract `FileStorage` port.
//! The concrete implementation lives here.

pub mod s3_file_storage;

pub use s3_file_storage::S3FileStorage;
