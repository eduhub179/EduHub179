//! Infrastructure crate: реализации репозиториев, адаптеры (Postgres, Redis, S3)

pub mod postgres;
pub mod redis;
pub mod storage;
pub mod auth;
pub mod notifications;
