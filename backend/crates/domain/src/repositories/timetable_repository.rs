//! Admin writes to the timetable that the per-entity repositories don't offer:
//! finding a template by its slot, re-pointing or deleting future lesson instances
//! when the regular timetable changes, moving homework with a moved lesson.
//!
//! Implementation of this trait is located in the `infrastructure` crate.

use chrono::{NaiveDate, NaiveTime};
use uuid::Uuid;

use crate::entities::lesson_template::LessonTemplate;
use crate::errors::DomainError;
use crate::value_objects::{day_of_week::DayOfWeek, week_parity::WeekParity};

/// A lesson instance with what the timetable editor needs to decide its fate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRef {
    pub instance_id: Uuid,
    pub week_start_date: NaiveDate,
    pub lesson_date: NaiveDate,
    pub cabinet_id: Option<Uuid>,
    pub has_homework: bool,
}

#[async_trait::async_trait]
pub trait TimetableRepository: Send + Sync {
    /// The template with exactly this slot (active or not), if any.
    async fn find_template(
        &self,
        lesson_id: Uuid,
        day: DayOfWeek,
        start_time: NaiveTime,
        end_time: NaiveTime,
        parity: WeekParity,
    ) -> Result<Option<LessonTemplate>, DomainError>;

    /// Instances of a template on or after `from`, by date.
    async fn instances_from(
        &self,
        template_id: Uuid,
        from: NaiveDate,
    ) -> Result<Vec<InstanceRef>, DomainError>;

    /// Moves an instance to another template (and date / cabinet), keeping its id,
    /// status, homework and substitution.
    async fn repoint_instance(
        &self,
        instance_id: Uuid,
        template_id: Uuid,
        lesson_date: NaiveDate,
        cabinet_id: Option<Uuid>,
    ) -> Result<(), DomainError>;

    /// Deletes an instance without homework (its substitution goes with it).
    async fn delete_instance(&self, instance_id: Uuid) -> Result<(), DomainError>;

    /// Moves the homework of one instance to another; no-op without homework.
    /// `HomeworkAlreadyExists` if the target already has homework.
    async fn move_homework(
        &self,
        from_instance: Uuid,
        to_instance: Uuid,
    ) -> Result<(), DomainError>;

    /// Student groups with at least one active student of the class.
    async fn class_group_ids(&self, class_id: Uuid) -> Result<Vec<Uuid>, DomainError>;
}
