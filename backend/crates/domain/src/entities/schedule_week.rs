//! ScheduleWeek entity — the week container (docs/SCHEDULE.en.md).
//!
//! The schedule-building model:
//! - rows of the grid: `schedule_weeks` (one row per week, `week_start_date` = Monday);
//! - columns are CONCEPTUAL (templates) — the physical cells are `lesson_instances`,
//!   which reference `(template_id, week_start_date)`;
//! - a week "knows" its attached instances through the FK, not stored lists.
//!
//! Invariants: none beyond typed fields — `week_start_date` and `copied_from`
//! are `NaiveDate`s, `status` is a typed VO. Hence a plain `new` constructor
//! (like `Lesson::new`), not `try_new`.

use crate::value_objects::week_status::WeekStatus;
use chrono::NaiveDate;

/// A row of the schedule grid: one school week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleWeek {
    /// Start of the week (Monday), the natural key of the table.
    pub week_start_date: NaiveDate,
    /// Lifecycle: draft (invisible to students) or published (visible).
    pub status: WeekStatus,
    /// Provenance: the week this one was copied from (NULL = generated
    /// from templates / manual).
    pub copied_from: Option<NaiveDate>,
}

impl ScheduleWeek {
    /// Constructor. Cannot fail — all fields are typed.
    pub fn new(
        week_start_date: NaiveDate,
        status: WeekStatus,
        copied_from: Option<NaiveDate>,
    ) -> Self {
        Self {
            week_start_date,
            status,
            copied_from,
        }
    }

    /// Convenience accessor, mirrors `WeekStatus::is_published`.
    pub fn is_published(&self) -> bool {
        self.status.is_published()
    }

    /// Convenience accessor, mirrors `WeekStatus::is_draft`.
    pub fn is_draft(&self) -> bool {
        self.status.is_draft()
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain schedule_week`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    #[test]
    fn new_published_week_succeeds() {
        let week = ScheduleWeek::new(d(2026, 9, 7), WeekStatus::Published, None);

        assert_eq!(week.week_start_date, d(2026, 9, 7));
        assert!(week.is_published());
        assert!(!week.is_draft());
        assert_eq!(week.copied_from, None);
    }

    #[test]
    fn new_draft_week_with_copied_from_succeeds() {
        let week = ScheduleWeek::new(
            d(2026, 9, 14),
            WeekStatus::Draft,
            Some(d(2026, 9, 7)),
        );

        assert!(week.is_draft());
        assert_eq!(week.copied_from, Some(d(2026, 9, 7)));
    }

    #[test]
    fn equality_is_by_all_fields() {
        let a = ScheduleWeek::new(d(2026, 9, 7), WeekStatus::Published, None);
        let b = ScheduleWeek::new(d(2026, 9, 7), WeekStatus::Published, None);
        assert_eq!(a, b);
    }

    #[test]
    fn weeks_differ_by_status() {
        let a = ScheduleWeek::new(d(2026, 9, 7), WeekStatus::Published, None);
        let b = ScheduleWeek::new(d(2026, 9, 7), WeekStatus::Draft, None);
        assert_ne!(a, b);
    }
}
