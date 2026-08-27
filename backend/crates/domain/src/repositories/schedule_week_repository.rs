//! Repository trait for schedule week persistence.
//!
//! Dependencies: Only types from `crate::entities`, `crate::value_objects` and
//! `crate::errors`. Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::schedule_week::ScheduleWeek;
use crate::errors::DomainError;
use chrono::NaiveDate;

/// Interface for interacting with `schedule_weeks` storage.
///
/// Weeks are the rows of the schedule grid (docs/SCHEDULE.en.md): the unit of
/// schedule building. Admin flow: create the week (draft) → fill it with
/// instances → publish. Students see only published weeks.
#[async_trait::async_trait]
pub trait ScheduleWeekRepository: Send + Sync {
    /// Fetches a week by its start date (the natural key).
    /// Fail-safe: Returns `ScheduleWeekNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, week_start_date: NaiveDate) -> Result<ScheduleWeek, DomainError>;

    /// Fetches all weeks, most recent first (admin view).
    async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError>;

    /// Saves or updates a week (atomic upsert on `week_start_date`).
    ///
    /// Publishing / re-drafting / setting `copied_from` are all done by saving
    /// the updated entity. Errors:
    /// - `ScheduleWeekNotFound` — `copied_from` references a missing week (FK
    ///   `schedule_weeks_copied_from_fkey`).
    async fn save(&self, week: ScheduleWeek) -> Result<ScheduleWeek, DomainError>;
}
