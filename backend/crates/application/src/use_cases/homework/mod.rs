//! Homework-related use cases.
//!
//! This module groups application flows for homework attachments and file management.
//! Planned scenarios:
//! - create homework
//! - update homework text
//! - upload homework file
//! - attach uploaded file to a homework
//! - attach a file to a lesson (upload + attach, creates the homework if needed)
//! - download homework file
//! - delete homework file
//! - delete homework with all its files
//!
//! Dependencies: `domain` crate (entities, repositories, ports, errors).
//! Guarantees: all flows return `Result` and stay independent from PostgreSQL/S3 details.

pub mod attach_file_to_lesson;
pub mod attach_to_homework;
pub mod create_homework;
pub mod delete_file;
pub mod delete_homework;
pub mod download_file;
pub mod update_homework_text;
pub mod upload_file;
