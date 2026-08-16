//! Repository trait for lesson template persistence.
//!
//! Dependencies: Only types from `crate::entities`, `crate::value_objects` and
//! `crate::errors`. Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::lesson_template::LessonTemplate;
use crate::errors::DomainError;
use crate::value_objects::day_of_week::DayOfWeek;
use uuid::Uuid;

/// Interface for interacting with lesson template storage.
///
/// Templates are the "sometimes changes" layer of the schedule (see
/// docs/SCHEDULE.en.md): the school-wide rhythm that week instances are
/// generated from. Used by schedule building (generate-from-templates,
/// copy-week) and availability checks.
#[async_trait::async_trait]
pub trait LessonTemplateRepository: Send + Sync {
    /// Fetches a template by its unique identifier.
    /// Fail-safe: Returns `LessonTemplateNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, template_id: Uuid) -> Result<LessonTemplate, DomainError>;

    /// Fetches ALL templates of a lesson (active and archived), ordered by
    /// (day, start_time). Used for the lesson's full history and for
    /// availability queries.
    async fn get_by_lesson(&self, lesson_id: Uuid) -> Result<Vec<LessonTemplate>, DomainError>;

    /// Fetches all ACTIVE templates on a given day, ordered by start_time.
    /// Used when building a week's schedule (a day is the building unit).
    /// Performance: relies on `idx_lesson_templates_day_active`.
    async fn get_active_for_day(&self, day: DayOfWeek) -> Result<Vec<LessonTemplate>, DomainError>;

    /// Fetches ALL active templates, ordered by (day, start_time).
    /// Used by generate-from-templates (docs/SCHEDULE.en.md §5.1).
    async fn get_all_active(&self) -> Result<Vec<LessonTemplate>, DomainError>;

    /// Saves or updates a template (atomic upsert on `template_id`).
    ///
    /// Updating day/time/cabinet is how a school-wide schedule change is made —
    /// instances join the CURRENT template state, so future weeks follow.
    /// Archiving is done by saving with `is_active = false`.
    ///
    /// Errors:
    /// - `LessonTemplateAlreadyExists` — a NEW template with the same
    ///   (lesson_id, day, start_time, end_time, parity) as an existing one
    ///   (dedup index `idx_lesson_templates_no_dup`).
    /// - `LessonNotFound` — `lesson_id` references a missing lesson (FK).
    /// - `CabinetNotFound` — `cabinet_id` references a missing cabinet (FK).
    async fn save(&self, template: LessonTemplate) -> Result<LessonTemplate, DomainError>;
}
