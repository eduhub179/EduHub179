//! Course materials use cases: topics, files and links of a course.
//!
//! Dependencies: `domain` crate (entities, repositories, ports), `LessonAccess`.
//! Guarantees: all flows return `Result` and stay independent from PostgreSQL / S3 details.

pub mod materials_service;

pub use materials_service::MaterialsService;
