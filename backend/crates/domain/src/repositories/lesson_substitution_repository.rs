//! Repository trait for lesson substitutions (substitute teacher, note) and the
//! "added by an admin" mark of a lesson.
//!
//! Implementation of this trait is located in the `infrastructure` crate.

use chrono::NaiveDate;
use uuid::Uuid;

use crate::entities::lesson_substitution::LessonSubstitution;
use crate::errors::DomainError;

#[async_trait::async_trait]
pub trait LessonSubstitutionRepository: Send + Sync {
    /// Substitution of one lesson instance; `None` if the lesson is unchanged.
    async fn get(&self, instance_id: Uuid) -> Result<Option<LessonSubstitution>, DomainError>;

    /// Whether an admin added this lesson to one date (one-off or moved lesson).
    async fn is_extra(&self, instance_id: Uuid) -> Result<bool, DomainError>;

    /// Instances in `[from, to]` where the teacher substitutes (for the teacher's schedule).
    async fn instance_ids_for_teacher(
        &self,
        teacher_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Uuid>, DomainError>;

    /// Upsert by `instance_id`.
    ///
    /// Errors: `LessonInstanceNotFound` for a missing instance, `UserNotFound`
    /// for a missing teacher or author.
    async fn save(&self, substitution: LessonSubstitution) -> Result<LessonSubstitution, DomainError>;

    /// Removes the substitution; no error if there was none.
    async fn delete(&self, instance_id: Uuid) -> Result<(), DomainError>;
}
