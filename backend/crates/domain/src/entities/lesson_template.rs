//! LessonTemplate entity (the "sometimes changes" layer of the schedule).
//!
//! Invariants:
//! - `end_time` must be strictly after `start_time` (DB CHECK `chk_template_time`).
//! - `day` and `parity` are typed value objects — invalid values cannot be constructed.
//! - `lesson_id` / `cabinet_id` reference existing rows (enforced by DB FKs).
//!
//! Dependencies: `crate::errors::DomainError`, `crate::value_objects::{day_of_week,
//! week_parity}`, `chrono::NaiveTime`, `uuid::Uuid`.
//! Guarantees: An instance can only be created via `try_new`, which validates
//! the invariants. This prevents invalid entities from reaching the repository.
//!
//! Substitutions are handled at the instance level (cancel original + create
//! replacement instance), not by templates.

use crate::errors::DomainError;
use crate::value_objects::day_of_week::DayOfWeek;
use crate::value_objects::week_parity::WeekParity;
use chrono::NaiveTime;
use uuid::Uuid;

/// A lesson template: "subject-time-cabinet" combination that is always valid.
///
/// Layer model (docs/SCHEDULE.en.md):
/// - `lessons` change extremely rarely (class/group + subject + teachers);
/// - `lesson_templates` change sometimes (school-wide schedule change);
/// - `lesson_instances` change each time (one row per template per week).
///
/// A template has exactly one day of the week, so "the lesson of week W" is fully
/// determined by (template, week_start_date).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonTemplate {
    /// Unique template identifier (UUID v4).
    pub id: Uuid,
    /// The lesson (class/group + subject + teachers) this template schedules.
    pub lesson_id: Uuid,
    /// Which day of the week the lesson takes place.
    pub day: DayOfWeek,
    /// Start time (TIME in DB).
    pub start_time: NaiveTime,
    /// End time (TIME in DB); must be strictly after `start_time`.
    pub end_time: NaiveTime,
    /// Periodicity (every / odd / even). Defaults to every week.
    pub parity: WeekParity,
    /// Where the lesson takes place (optional; NULL = not assigned).
    pub cabinet_id: Option<Uuid>,
    /// Activity flag. Inactive (archived) templates do not participate in
    /// availability checks and schedule building, but instances referencing
    /// them stay readable (soft-archive pattern).
    pub is_active: bool,
}

impl LessonTemplate {
    /// Constructor with invariant validation (fail-safe).
    ///
    /// Returns `Err` if:
    /// - `end_time <= start_time` → `InvalidLessonTemplateTime`.
    pub fn try_new(
        id: Uuid,
        lesson_id: Uuid,
        day: DayOfWeek,
        start_time: NaiveTime,
        end_time: NaiveTime,
        parity: WeekParity,
        cabinet_id: Option<Uuid>,
        is_active: bool,
    ) -> Result<Self, DomainError> {
        if end_time <= start_time {
            return Err(DomainError::InvalidLessonTemplateTime);
        }
        Ok(Self {
            id,
            lesson_id,
            day,
            start_time,
            end_time,
            parity,
            cabinet_id,
            is_active,
        })
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain lesson_template`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).expect("valid time")
    }

    #[test]
    fn try_new_valid_template_succeeds() {
        let id = Uuid::new_v4();
        let lesson_id = Uuid::new_v4();
        let cabinet_id = Uuid::new_v4();

        let template = LessonTemplate::try_new(
            id,
            lesson_id,
            DayOfWeek::Mon,
            t(9, 0),
            t(9, 45),
            WeekParity::Every,
            Some(cabinet_id),
            true,
        )
        .expect("valid template");

        assert_eq!(template.id, id);
        assert_eq!(template.lesson_id, lesson_id);
        assert_eq!(template.day, DayOfWeek::Mon);
        assert_eq!(template.start_time, t(9, 0));
        assert_eq!(template.end_time, t(9, 45));
        assert_eq!(template.parity, WeekParity::Every);
        assert_eq!(template.cabinet_id, Some(cabinet_id));
        assert!(template.is_active);
    }

    #[test]
    fn try_new_without_cabinet_succeeds() {
        let template = LessonTemplate::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            DayOfWeek::Sat,
            t(10, 0),
            t(10, 40),
            WeekParity::Odd,
            None,
            true,
        )
        .expect("valid template");

        assert_eq!(template.cabinet_id, None);
        assert_eq!(template.parity, WeekParity::Odd);
    }

    #[test]
    fn end_equals_start_is_rejected() {
        let err = LessonTemplate::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            DayOfWeek::Mon,
            t(9, 0),
            t(9, 0),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidLessonTemplateTime);
    }

    #[test]
    fn end_before_start_is_rejected() {
        let err = LessonTemplate::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            DayOfWeek::Mon,
            t(10, 0),
            t(9, 0),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap_err();

        assert_eq!(err, DomainError::InvalidLessonTemplateTime);
    }

    #[test]
    fn equality_is_by_all_fields() {
        let id = Uuid::new_v4();
        let lesson_id = Uuid::new_v4();
        let a = LessonTemplate::try_new(
            id,
            lesson_id,
            DayOfWeek::Tue,
            t(11, 0),
            t(11, 45),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap();
        let b = LessonTemplate::try_new(
            id,
            lesson_id,
            DayOfWeek::Tue,
            t(11, 0),
            t(11, 45),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap();

        assert_eq!(a, b);
    }

    #[test]
    fn templates_differ_by_is_active() {
        let a = LessonTemplate::try_new(
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
        let b = LessonTemplate::try_new(
            a.id,
            a.lesson_id,
            a.day,
            a.start_time,
            a.end_time,
            a.parity,
            a.cabinet_id,
            false,
        )
        .unwrap();

        assert_ne!(a, b);
    }
}
