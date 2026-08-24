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
//! - `lesson_date` must be Monday–Saturday. Templates have no Sunday
//!   (`day_of_week` ends at 'sat'), so a lesson instance cannot land on Sunday
//!   either — Sunday belongs to events. Prefer `for_template`, which derives
//!   the date from the template's day and cannot produce a mismatch.
//! - `status` and `cabinet_id` are typed/optional — invalid values cannot be constructed.

use crate::entities::lesson_template::LessonTemplate;
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
        // No lessons on Sunday: the day_of_week enum ends at 'sat', so a
        // template can never produce a Sunday lesson. Reject it here too,
        // so a Sunday date cannot sneak in through manual/copy paths.
        if lesson_date.weekday().num_days_from_monday() > 5 {
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

    /// Builds the instance a template produces in a given week.
    ///
    /// The lesson date is DERIVED from the template's day
    /// (`week_start_date + template.day`), never passed in — a Monday template
    /// cannot produce a Tuesday instance, and a Sunday instance is impossible
    /// by construction. This is the constructor schedule building must use.
    /// Infallible: `template.day` is a valid `DayOfWeek` (Mon–Sat), so the
    /// derived date always falls inside the week.
    pub fn for_template(
        id: Uuid,
        template: &LessonTemplate,
        week_start_date: NaiveDate,
        status: LessonInstanceStatus,
        cabinet_id: Option<Uuid>,
    ) -> Self {
        let lesson_date = week_start_date
            + chrono::Duration::days(i64::from(template.day.num_days_from_monday()));
        Self {
            id,
            template_id: template.id,
            week_start_date,
            lesson_date,
            status,
            cabinet_id,
        }
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
    use crate::value_objects::{day_of_week::DayOfWeek, week_parity::WeekParity};
    use chrono::{NaiveDate, NaiveTime};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).expect("valid time")
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
        // Week starts Monday 2026-09-07; Saturday 2026-09-12 is the last valid
        // day — Sunday belongs to events, not lessons.
        let instance = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 12),
            LessonInstanceStatus::Cancelled,
            None,
        )
        .expect("Saturday is within the week");

        assert!(instance.status.is_cancelled());
        assert_eq!(instance.cabinet_id, None);
    }

    #[test]
    fn try_new_sunday_is_rejected() {
        // Sunday 2026-09-13 is inside the week window but is not a lesson day.
        let err = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            d(2026, 9, 7),
            d(2026, 9, 13),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidLessonInstanceDate);
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
    fn for_template_derives_monday_date() {
        let template = LessonTemplate::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            DayOfWeek::Mon,
            t(9, 0),
            t(9, 45),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap();

        let instance = LessonInstance::for_template(
            Uuid::new_v4(),
            &template,
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            None,
        );

        assert_eq!(instance.lesson_date, d(2026, 9, 7), "Mon = week start");
        assert_eq!(instance.day_of_week(), 0);
        assert_eq!(instance.template_id, template.id);
    }

    #[test]
    fn for_template_derives_saturday_date() {
        let template = LessonTemplate::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            DayOfWeek::Sat,
            t(10, 0),
            t(10, 40),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap();

        let instance = LessonInstance::for_template(
            Uuid::new_v4(),
            &template,
            d(2026, 9, 7),
            LessonInstanceStatus::Scheduled,
            None,
        );

        assert_eq!(instance.lesson_date, d(2026, 9, 12), "Sat = week start + 5");
        assert_eq!(instance.day_of_week(), 5);
    }

    #[test]
    fn for_template_is_infallible_for_every_day() {
        // Every valid template day yields a date inside the week — this is the
        // point of deriving instead of passing the date.
        for (day, expected) in [
            (DayOfWeek::Mon, d(2026, 9, 7)),
            (DayOfWeek::Tue, d(2026, 9, 8)),
            (DayOfWeek::Wed, d(2026, 9, 9)),
            (DayOfWeek::Thu, d(2026, 9, 10)),
            (DayOfWeek::Fri, d(2026, 9, 11)),
            (DayOfWeek::Sat, d(2026, 9, 12)),
        ] {
            let template = LessonTemplate::try_new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                day,
                t(9, 0),
                t(9, 45),
                WeekParity::Every,
                None,
                true,
            )
            .unwrap();
            let instance = LessonInstance::for_template(
                Uuid::new_v4(),
                &template,
                d(2026, 9, 7),
                LessonInstanceStatus::Scheduled,
                None,
            );
            assert_eq!(instance.lesson_date, expected, "derived date for {day:?}");
        }
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
