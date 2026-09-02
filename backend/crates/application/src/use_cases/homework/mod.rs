//! Homework-related use cases.
//!
//! This module groups application flows for homework attachments and file management.
//! Planned scenarios:
//! - upload homework file
//! - attach uploaded file to a homework
//! - delete homework file
//!
//! Dependencies: `domain` crate (entities, repositories, ports, errors).
//! Guarantees: all flows return `Result` and stay independent from PostgreSQL/S3 details.

pub mod attach_to_homework;
pub mod delete_file;
pub mod upload_file;
