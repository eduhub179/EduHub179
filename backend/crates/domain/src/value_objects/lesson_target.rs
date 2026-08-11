//! Value Object: group or class
//!
//! Corresponds to the (class_id, group_id) nullable column pair in the
//! `lessons` table with the CHECK constraint `chk_one_entity`.
//! Guarantees: XOR is enforced by the type system — it is impossible to
//! construct a target with both entities or with neither. Any conversion
//! from a DB row that violates this invariant returns `Err`, preventing
//! "garbage" rows from entering the domain.
use uuid::Uuid;

/// The target of a lesson: either a whole class or a student group.
///
/// Examples:
/// - `LessonTarget::Class(id)` → "Спецмат в 10б" (урок для всего класса)
/// - `LessonTarget::Group(id)` → "Английский B1" (группа из разных классов)
///
/// This is a Value Object: it is immutable, has no identity of its own,
/// and equality is purely structural.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LessonTarget {
    /// Lesson for the whole class.
    Class(Uuid),
    /// Lesson for a student group, which may span multiple classes.
    Group(Uuid),
}

impl LessonTarget {
    /// Constructs a class-target from a DB column pair.
    ///
    /// Fail-safe: returns `None` unless exactly one of the two IDs is present,
    /// mirroring the CHECK constraint `chk_one_entity` on the `lessons` table.
    /// Used by the infrastructure layer when mapping rows to the domain.
    pub fn from_db(class_id: Option<Uuid>, group_id: Option<Uuid>) -> Option<Self> {
        match (class_id, group_id) {
            (Some(id), None) => Some(LessonTarget::Class(id)),
            (None, Some(id)) => Some(LessonTarget::Group(id)),
            _ => None, // both set or both NULL → invariant violation in DB
        }
    }

    /// Returns the class ID if this target is a class, otherwise `None`.
    pub fn class_id(&self) -> Option<Uuid> {
        match self {
            LessonTarget::Class(id) => Some(*id),
            LessonTarget::Group(_) => None,
        }
    }

    /// Returns the group ID if this target is a group, otherwise `None`.
    pub fn group_id(&self) -> Option<Uuid> {
        match self {
            LessonTarget::Class(_) => None,
            LessonTarget::Group(id) => Some(*id),
        }
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain lesson_target`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_target_exposes_class_id_only() {
        let id = Uuid::new_v4();
        let target = LessonTarget::Class(id);
        assert_eq!(target.class_id(), Some(id));
        assert_eq!(target.group_id(), None);
    }

    #[test]
    fn group_target_exposes_group_id_only() {
        let id = Uuid::new_v4();
        let target = LessonTarget::Group(id);
        assert_eq!(target.group_id(), Some(id));
        assert_eq!(target.class_id(), None);
    }

    /// Важно: `Class(x)` и `Group(x)` с одним и тем же UUID — это РАЗНЫЕ цели.
    #[test]
    fn class_and_group_with_same_uuid_are_not_equal() {
        let id = Uuid::new_v4();
        assert_ne!(LessonTarget::Class(id), LessonTarget::Group(id));
    }

    #[test]
    fn equality_is_structural() {
        let id = Uuid::new_v4();
        assert_eq!(LessonTarget::Class(id), LessonTarget::Class(id));
        assert_eq!(LessonTarget::Group(id), LessonTarget::Group(id));
    }

    // === from_db: инвариант "ровно одно из двух" ===

    #[test]
    fn from_db_class_only_succeeds() {
        let id = Uuid::new_v4();
        assert_eq!(
            LessonTarget::from_db(Some(id), None),
            Some(LessonTarget::Class(id))
        );
    }

    #[test]
    fn from_db_group_only_succeeds() {
        let id = Uuid::new_v4();
        assert_eq!(
            LessonTarget::from_db(None, Some(id)),
            Some(LessonTarget::Group(id))
        );
    }

    #[test]
    fn from_db_both_null_fails() {
        assert_eq!(LessonTarget::from_db(None, None), None);
    }

    #[test]
    fn from_db_both_set_fails() {
        let class_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        assert_eq!(LessonTarget::from_db(Some(class_id), Some(group_id)), None);
    }
}
