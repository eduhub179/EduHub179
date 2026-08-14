//! Lesson entity.
//!
//! Invariants:
//! - Exactly one of class / group is set as the lesson target
//!   (enforced by the `LessonTarget` value object).
//! - `subject_id` references a valid subject (enforced by DB FK).
//!
//! Dependencies: `crate::errors::DomainError`, `crate::value_objects::lesson_target`,
//! `uuid::Uuid`.
//! Guarantees: The XOR invariant (class ИЛИ group) is guaranteed at compile time
//! by the `LessonTarget` type, mirroring the DB CHECK constraint `chk_one_entity`.
use crate::value_objects::lesson_target::LessonTarget;
use uuid::Uuid;

/// Representation of an abstract lesson.
///
/// A lesson is the combination of (class OR group) + subject.
/// It is "abstract" because it is not tied to a specific date — scheduling
/// (`lesson_templates` → `lesson_instances`) builds on top of it,
/// and homework / plusnik reference it either directly (plusnik) or via instances (homework).
///
/// Examples: "Спецмат в 10б", "Английский B1 (группа)".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lesson {
    /// Unique lesson identifier (UUID v4).
    pub id: Uuid,
    /// The target of the lesson (class or group). Exactly one — guaranteed by the VO.
    pub target: LessonTarget,
    /// The subject taught in this lesson.
    pub subject_id: Uuid,
    /// Activity flag. Inactive lessons are hidden from active schedules
    /// but retain their history (homework, plusnik) — soft-delete pattern.
    pub is_active: bool,
}

impl Lesson {
    /// Constructor.
    ///
    /// The XOR invariant (exactly one of class/group) is enforced at compile time
    /// by `LessonTarget`, so no runtime validation is required.
    /// Unlike `try_new` in other entities, this cannot fail — hence no `Result`.
    pub fn new(id: Uuid, target: LessonTarget, subject_id: Uuid, is_active: bool) -> Self {
        Self {
            id,
            target,
            subject_id,
            is_active,
        }
    }

    /// Convenience accessor: the class ID if this lesson targets a class.
    pub fn class_id(&self) -> Option<Uuid> {
        self.target.class_id()
    }

    /// Convenience accessor: the group ID if this lesson targets a group.
    pub fn group_id(&self) -> Option<Uuid> {
        self.target.group_id()
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain lesson`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_class_target_succeeds() {
        let id = Uuid::new_v4();
        let class_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let lesson = Lesson::new(id, LessonTarget::Class(class_id), subject_id, true);

        assert_eq!(lesson.id, id);
        assert_eq!(lesson.target, LessonTarget::Class(class_id));
        assert_eq!(lesson.subject_id, subject_id);
        assert!(lesson.is_active);
    }

    #[test]
    fn new_with_group_target_succeeds() {
        let group_id = Uuid::new_v4();
        let lesson = Lesson::new(
            Uuid::new_v4(),
            LessonTarget::Group(group_id),
            Uuid::new_v4(),
            true,
        );

        assert_eq!(lesson.target, LessonTarget::Group(group_id));
    }

    #[test]
    fn accessors_delegate_to_target() {
        let class_id = Uuid::new_v4();
        let lesson = Lesson::new(
            Uuid::new_v4(),
            LessonTarget::Class(class_id),
            Uuid::new_v4(),
            true,
        );

        assert_eq!(lesson.class_id(), Some(class_id));
        assert_eq!(lesson.group_id(), None);
    }

    #[test]
    fn equality_is_by_all_fields() {
        let id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();
        let target = LessonTarget::Class(Uuid::new_v4());

        let a = Lesson::new(id, target, subject_id, true);
        let b = Lesson::new(id, target, subject_id, true);

        assert_eq!(a, b);
    }

    #[test]
    fn lessons_differ_by_is_active() {
        let id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();
        let target = LessonTarget::Group(Uuid::new_v4());

        let active = Lesson::new(id, target, subject_id, true);
        let inactive = Lesson::new(id, target, subject_id, false);

        assert_ne!(active, inactive);
    }
}
