//! Admin writes to the timetable that touch several rows at once: adding, changing
//! and removing a regular slot together with the lessons of already created weeks,
//! placing a one-off or moved lesson.
//!
//! Every write runs in ONE database transaction under a timetable-wide lock: a failure
//! in the middle leaves nothing half-done, and two admins' edits never interleave
//! (the "is the lesson already there / does it have homework" checks happen inside
//! the transaction, on locked rows).
//!
//! Implementation of this trait is located in the `infrastructure` crate.

use chrono::{NaiveDate, NaiveTime};
use uuid::Uuid;

use crate::entities::lesson_template::LessonTemplate;
use crate::errors::DomainError;
use crate::value_objects::{day_of_week::DayOfWeek, week_parity::WeekParity};

/// Time and place of a slot: a regular one or that of a one-off lesson.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotSpec {
    pub lesson_id: Uuid,
    pub day: DayOfWeek,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub parity: WeekParity,
    pub cabinet_id: Option<Uuid>,
}

/// What a regular timetable write did to already created weeks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteCount {
    /// Lessons added, moved or removed in existing weeks.
    pub changed: usize,
    /// Lessons left as they were because they have homework (removed slots: cancelled instead).
    pub kept: usize,
}

#[async_trait::async_trait]
pub trait TimetableRepository: Send + Sync {
    /// Adds an active slot, reusing a deactivated template with the same slot. With
    /// `apply_from`, the lesson is also added to existing weeks from that date on.
    ///
    /// Errors: `LessonTemplateAlreadyExists` for the same course at the same time,
    /// `LessonTemplateSlotConflict` for an overlapping slot of the same course.
    async fn add_slot(
        &self,
        slot: SlotSpec,
        apply_from: Option<NaiveDate>,
    ) -> Result<(LessonTemplate, WriteCount), DomainError>;

    /// Changes a slot. Only the cabinet changed: the template keeps its id, and lessons
    /// from `apply_from` on that used the old cabinet get the new one. Otherwise the old
    /// template is deactivated, the new slot is added as in `add_slot`, and lessons from
    /// `apply_from` on move to it, except those already taken in that week, those that
    /// would land before `apply_from`, and those with homework when the course changes.
    ///
    /// Errors: as `add_slot`, plus `LessonTemplateNotFound`.
    async fn change_slot(
        &self,
        template_id: Uuid,
        slot: SlotSpec,
        apply_from: Option<NaiveDate>,
    ) -> Result<WriteCount, DomainError>;

    /// Deactivates a slot. With `apply_from`, its lessons from that date on are deleted;
    /// those with homework are cancelled instead.
    ///
    /// Errors: `LessonTemplateNotFound`.
    async fn remove_slot(
        &self,
        template_id: Uuid,
        apply_from: Option<NaiveDate>,
    ) -> Result<WriteCount, DomainError>;

    /// Puts a lesson of `slot` on `date` of the week `week_start_date` and marks it as
    /// added (`is_extra`) unless the slot is a regular active one. A new slot gets an
    /// inactive template; a cancelled lesson in the slot is restored, not duplicated.
    /// With `moved_from`, that lesson is cancelled and its homework moves to the new one
    /// (unless the new one already has homework: then both keep theirs).
    ///
    /// Returns the instance id. Errors: `LessonInstanceAlreadyExists` if the lesson is
    /// already there, `LessonInstanceNotFound` if `moved_from` is missing or cancelled,
    /// `ScheduleWeekNotFound` without the week.
    async fn place_lesson(
        &self,
        slot: SlotSpec,
        week_start_date: NaiveDate,
        date: NaiveDate,
        moved_from: Option<Uuid>,
    ) -> Result<Uuid, DomainError>;

    /// Student groups with at least one active student of the class.
    async fn class_group_ids(&self, class_id: Uuid) -> Result<Vec<Uuid>, DomainError>;
}
