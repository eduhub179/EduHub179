//! Value Object for the lesson instance status.
//!
//! Corresponds to the `lesson_instances.status` column
//! ('scheduled', 'completed', 'cancelled').
//! Display rules (docs/OVERRIDES.en.md §7): cancelled instances are returned
//! to the client so they can be rendered greyed — nothing auto-shadows.

use crate::errors::DomainError;
use std::str::FromStr;

/// Status of a concrete lesson occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LessonInstanceStatus {
    /// The lesson is on the schedule and will take place.
    Scheduled,
    /// The lesson has already happened.
    Completed,
    /// The lesson was cancelled (shown greyed to students).
    Cancelled,
}

impl LessonInstanceStatus {
    /// Fail-safe: explicit check for the scheduled state.
    pub fn is_scheduled(&self) -> bool {
        matches!(self, LessonInstanceStatus::Scheduled)
    }

    /// Fail-safe: explicit check for the completed state.
    pub fn is_completed(&self) -> bool {
        matches!(self, LessonInstanceStatus::Completed)
    }

    /// Fail-safe: explicit check for the cancelled state.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, LessonInstanceStatus::Cancelled)
    }
}

impl FromStr for LessonInstanceStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "scheduled" => Ok(LessonInstanceStatus::Scheduled),
            "completed" => Ok(LessonInstanceStatus::Completed),
            "cancelled" => Ok(LessonInstanceStatus::Cancelled),
            _ => Err(DomainError::InvalidLessonInstanceStatus),
        }
    }
}

impl std::fmt::Display for LessonInstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LessonInstanceStatus::Scheduled => "scheduled",
            LessonInstanceStatus::Completed => "completed",
            LessonInstanceStatus::Cancelled => "cancelled",
        };
        write!(f, "{}", s)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain lesson_instance_status`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_values_parse() {
        for (s, expected) in [
            ("scheduled", LessonInstanceStatus::Scheduled),
            ("completed", LessonInstanceStatus::Completed),
            ("cancelled", LessonInstanceStatus::Cancelled),
        ] {
            assert_eq!(s.parse::<LessonInstanceStatus>().unwrap(), expected, "{s}");
        }
    }

    #[test]
    fn unknown_value_is_rejected() {
        assert_eq!(
            "postponed".parse::<LessonInstanceStatus>(),
            Err(DomainError::InvalidLessonInstanceStatus)
        );
        assert_eq!(
            "".parse::<LessonInstanceStatus>(),
            Err(DomainError::InvalidLessonInstanceStatus)
        );
    }

    #[test]
    fn helpers_match_variants() {
        assert!(LessonInstanceStatus::Scheduled.is_scheduled());
        assert!(LessonInstanceStatus::Completed.is_completed());
        assert!(LessonInstanceStatus::Cancelled.is_cancelled());
    }

    #[test]
    fn display_matches_db_values() {
        assert_eq!(LessonInstanceStatus::Scheduled.to_string(), "scheduled");
        assert_eq!(LessonInstanceStatus::Completed.to_string(), "completed");
        assert_eq!(LessonInstanceStatus::Cancelled.to_string(), "cancelled");
    }
}
