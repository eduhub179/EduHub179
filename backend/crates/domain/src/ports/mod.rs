//! Ports (interfaces) for external dependencies implemented by the
//! infrastructure layer.
//!
//! Placed in the domain crate (not application) so that `infrastructure`
//! can implement them without importing `application` (per dependency rules).
//!
//! Organized by feature: authentication lives in `auth/`; future ports
//! (clock, file storage, notifications) get their own files here.

pub mod auth;
pub mod file_storage;
