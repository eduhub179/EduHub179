//! Infrastructure crate: repository implementations, adapters (Postgres, Redis, S3).
//!
//! Guarantees:
//! - Implements traits defined in the `domain` crate.
//! - No business logic here — only data access and external service integration.
//! - All errors are mapped to `DomainError` before crossing the layer boundary.

pub mod auth;
pub mod postgres;
pub mod storage;
// pub mod redis;
// pub mod auth;
// pub mod notifications;
