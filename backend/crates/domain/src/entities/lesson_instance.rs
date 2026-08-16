//! LessonInstance entity — a cell of the schedule grid (docs/SCHEDULE.en.md).
//!
//! The most frequently changing layer: one row per (template, week) — the
//! concrete lesson on a concrete date. Homework/files point at instances as a
//! stable single pointer (instead of a composite (week, template) pair).
//!
//! Invariants:
//! - `lesson_date` must fall within the week: [week_start_date, week_start_date + 7)
//!   (mirrors the "computed from week_start_date + day" convention; guarded here
//!   because nothing in the DB enforces it).
//! - `status` and `cabinet_id` are typed/optional — invalid values cannot be constructed.

use crate::errors::DomainError;
use crate::value_objects::lesson_instance_status::LessonInstanceStatus;
use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

/// A concrete lesson occurrence on a concrete date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonInstance {
    /// Unique instance identifier (UUID v4).
    pub id: Uuid,
    /// The template this instance was generated from (the slot identity).
    pub template_id: Uuid,
    /// Start of the week this lesson belongs to (FK → schedule_weeks).
    pub week_start_date: NaiveDate,
    /// The concrete date of the lesson (within the week).
    pub lesson_date: NaiveDate,
    /// Status: scheduled / completed / cancelled.
    pub status: LessonInstanceStatus,
    /// Where the lesson actually takes place (NULL = not assigned yet).
    pub cabinet_id: Option<Uuid>,
}

impl LessonInstance {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Returns `Err` if `lesson_date` is not within
    /// [week_start_date, week_start_date + 7) → `InvalidLessonInstanceDate`.
    pub fn try_new(
        id: Uuid,
        template_id: Uuid,
        week_start_date: NaiveDate,
        lesson_date: NaiveDate,
        status: LessonInstanceStatus,
        cabinet_id: Option<Uuid>,
    ) -> Result<Self, DomainError> {
        let week_end = week_start_date + chrono::Duration::days(7);
        if lesson_date < week_start_date || lesson_date >= week_end {
            return Err(DomainError::InvalidLessonInstanceDate);
        }
        Ok(Self {
            id,
            template_id,
            week_start_date,
            lesson_date,
            status,
            cabinet_id,
        })
    }

    /// Convenience accessor for the day of week of this instance.
    pub fn day_of_week(&self) -> u32 {
        self.lesson_date.weekday().num_days_from_monday()
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain lesson_instance`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    #[test]
    fn try_new_valid_instance_succeeds() {
        let id = Uuid::new_v4();
        let template_id = Uuid::new_v4();
        let cabinet_id = Uuid::new_v4();

        let instance = LessonInstance::try_new(
            id,
            template_id,
            d(2026, 9, 7),
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            Some(cabinet_id),
        )
        .expect("valid instance");

        assert_eq!(instance.id, id);
        assert_eq!(instance.template_id, template_id);
        assert_eq!(instance.week_start_date, d(2026, 9, 7));
        assert_eq!(instance.lesson_date, d(2026, 9, 7));
        assert_eq!(instance.status, LessonInstanceStatus::Scheduled);
        assert_eq!(instance.cabinet_id, Some(cabinet_id));
    }

    #[test]
    fn try_new_last_day_of_week_succeeds() {
        // Week starts Monday 2026-09-07; Sunday 2026-09-13 is the last valid day.
        let instance = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 13),
            LessonInstanceStatus::Cancelled,
            None,
        )
        .expect("Sunday is within the week");

        assert!(instance.status.is_cancelled());
        assert_eq!(instance.cabinet_id, None);
    }

    #[test]
    fn try_new_before_week_start_is_rejected() {
        let err = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 6),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidLessonInstanceDate);
    }

    #[test]
    fn try_new_after_week_end_is_rejected() {
        // Monday 2026-09-14 is the start of the NEXT week.
        let err = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 14),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidLessonInstanceDate);
    }

    #[test]
    fn day_of_week_is_monday_based() {
        let instance = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 10), // Thursday
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();

        assert_eq!(instance.day_of_week(), 3);
    }

    #[test]
    fn equality_is_by_all_fields() {
        let id = Uuid::new_v4();
        let template_id = Uuid::new_v4();
        let a = LessonInstance::try_new(
            id,
            template_id,
            d(2026, 9, 7),
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();
        let b = LessonInstance::try_new(
            id,
            template_id,
            d(2026, 9, 7),
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();

        assert_eq!(a, b);
    }

    #[test]
    fn instances_differ_by_status() {
        let a = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();
        let b = LessonInstance::try_new(
            a.id,
            a.template_id,
            a.week_start_date,
            a.lesson_date,
            LessonInstanceStatus::Completed,
            a.cabinet_id,
        )
        .unwrap();

        assert_ne!(a, b);
    }
}
