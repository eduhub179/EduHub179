//! Repository trait for lesson instance persistence.
//!
//! Dependencies: Only types from `crate::entities`, `crate::value_objects` and
//! `crate::errors`. Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::lesson_instance::LessonInstance;
use crate::errors::DomainError;
use chrono::NaiveDate;
use uuid::Uuid;

/// Interface for interacting with `lesson_instances` storage.
///
/// Instances are the cells of the schedule grid (docs/SCHEDULE.en.md): the
/// most frequently changing layer, one row per (template, week). Homework and
/// files point at instances as a stable single pointer. Overrides (future)
/// attach to instances.
#[async_trait::async_trait]
pub trait LessonInstanceRepository: Send + Sync {
    /// Fetches an instance by its unique identifier.
    /// Fail-safe: Returns `LessonInstanceNotFound` if the record doesn't exist.
    async fn get_by_id(&self, instance_id: Uuid) -> Result<LessonInstance, DomainError>;

    /// Fetches all instances of a week, ordered by lesson_date.
    /// A week "knows" its cells through this query.
    async fn get_by_week(&self, week_start_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError>;

    /// Fetches all instances on a concrete date (the student schedule backbone).
    /// Performance: relies on `idx_lesson_instances_date`.
    async fn get_by_date(&self, lesson_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError>;

    /// Fetches all instances generated from a template, ordered by week_start_date.
    async fn get_by_template(&self, template_id: Uuid) -> Result<Vec<LessonInstance>, DomainError>;

    /// Saves or updates an instance (atomic upsert on `instance_id`).
    ///
    /// Changing status (cancel/complete) or the cabinet is done by saving the
    /// updated entity. Errors:
    /// - `LessonInstanceAlreadyExists` — a NEW instance with the same
    ///   (template_id, week_start_date) as an existing one (unique index
    ///   `idx_lesson_instances_unique` — one cell per template per week).
    /// - `LessonTemplateNotFound` — `template_id` references a missing template (FK).
    /// - `ScheduleWeekNotFound` — `week_start_date` references a missing week (FK —
    ///   create the week before its instances).
    /// - `CabinetNotFound` — `cabinet_id` references a missing cabinet (FK).
    async fn save(&self, instance: LessonInstance) -> Result<LessonInstance, DomainError>;
}
