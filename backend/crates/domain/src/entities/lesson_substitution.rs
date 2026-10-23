//! Lesson substitution: a change an admin makes to one concrete lesson instance
//! beyond what `lesson_instances` stores (status, cabinet).
//!
//! Invariants:
//! - At least one of `substitute_teacher_id` and `note` is set (otherwise there is
//!   no substitution and the row must be deleted instead).
//! - `note`, when present, is 1..=500 chars after trim.
//!
//! Dependencies: `crate::errors::DomainError`, `uuid`.
use crate::errors::DomainError;
use uuid::Uuid;

const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonSubstitution {
    pub instance_id: Uuid,
    /// Teacher who gives the lesson instead of the usual ones.
    pub substitute_teacher_id: Option<Uuid>,
    /// Shown to students next to the lesson.
    pub note: Option<String>,
    /// Admin who made the last change.
    pub updated_by: Uuid,
}

impl LessonSubstitution {
    /// Returns `Ok(None)` when both the teacher and the note are empty: there is
    /// nothing to store, and the caller should delete the substitution.
    pub fn try_new(
        instance_id: Uuid,
        substitute_teacher_id: Option<Uuid>,
        note: Option<String>,
        updated_by: Uuid,
    ) -> Result<Option<Self>, DomainError> {
        let note = match note.map(|n| n.trim().to_string()) {
            Some(n) if n.is_empty() => None,
            Some(n) if n.chars().count() > MAX_NOTE_CHARS => {
                return Err(DomainError::InvalidSubstitutionNote)
            }
            other => other,
        };
        if substitute_teacher_id.is_none() && note.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            instance_id,
            substitute_teacher_id,
            note,
            updated_by,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_substitution_is_none() {
        let s = LessonSubstitution::try_new(Uuid::new_v4(), None, Some("  ".into()), Uuid::new_v4());
        assert_eq!(s, Ok(None));
    }

    #[test]
    fn note_is_trimmed() {
        let s = LessonSubstitution::try_new(Uuid::new_v4(), None, Some(" в актовом зале ".into()), Uuid::new_v4())
            .unwrap()
            .unwrap();
        assert_eq!(s.note.as_deref(), Some("в актовом зале"));
    }

    #[test]
    fn long_note_is_rejected() {
        let s = LessonSubstitution::try_new(Uuid::new_v4(), None, Some("а".repeat(501)), Uuid::new_v4());
        assert_eq!(s, Err(DomainError::InvalidSubstitutionNote));
    }

    #[test]
    fn teacher_without_note_is_kept() {
        let teacher = Uuid::new_v4();
        let s = LessonSubstitution::try_new(Uuid::new_v4(), Some(teacher), None, Uuid::new_v4())
            .unwrap()
            .unwrap();
        assert_eq!(s.substitute_teacher_id, Some(teacher));
    }
}
