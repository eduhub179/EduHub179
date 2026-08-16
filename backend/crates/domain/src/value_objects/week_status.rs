//! Value Object for the schedule week lifecycle status.
//!
//! Corresponds to the `schedule_weeks.status` column ('draft', 'published').
//! Rules (docs/SCHEDULE.en.md §4):
//! - Students see instances only in PUBLISHED weeks.
//! - Availability checks see ALL weeks (drafts included) so building
//!   prevents conflicts before a week goes live.
//! - Hybrid edit rule (decided 2026-08-16): re-draft allowed only for weeks
//!   that have not started yet; after that, live-edit only.

use crate::errors::DomainError;
use std::str::FromStr;

/// Lifecycle status of a schedule week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeekStatus {
    /// Admin is still building the week; invisible to students.
    Draft,
    /// The week is final; students can see it.
    Published,
}

impl WeekStatus {
    /// Fail-safe: explicit check for the published state.
    pub fn is_published(&self) -> bool {
        matches!(self, WeekStatus::Published)
    }

    /// Fail-safe: explicit check for the draft state.
    pub fn is_draft(&self) -> bool {
        matches!(self, WeekStatus::Draft)
    }
}

impl FromStr for WeekStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(WeekStatus::Draft),
            "published" => Ok(WeekStatus::Published),
            _ => Err(DomainError::InvalidWeekStatus),
        }
    }
}

impl std::fmt::Display for WeekStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WeekStatus::Draft => "draft",
            WeekStatus::Published => "published",
        };
        write!(f, "{}", s)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain week_status`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_values_parse() {
        assert_eq!("draft".parse::<WeekStatus>().unwrap(), WeekStatus::Draft);
        assert_eq!(
            "published".parse::<WeekStatus>().unwrap(),
            WeekStatus::Published
        );
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!(
            "PUBLISHED".parse::<WeekStatus>().unwrap(),
            WeekStatus::Published
        );
    }

    #[test]
    fn unknown_value_is_rejected() {
        assert_eq!(
            "archived".parse::<WeekStatus>(),
            Err(DomainError::InvalidWeekStatus)
        );
        assert_eq!("".parse::<WeekStatus>(), Err(DomainError::InvalidWeekStatus));
    }

    #[test]
    fn helpers_match_variants() {
        assert!(WeekStatus::Published.is_published());
        assert!(!WeekStatus::Published.is_draft());
        assert!(WeekStatus::Draft.is_draft());
        assert!(!WeekStatus::Draft.is_published());
    }

    #[test]
    fn display_matches_db_values() {
        assert_eq!(WeekStatus::Draft.to_string(), "draft");
        assert_eq!(WeekStatus::Published.to_string(), "published");
    }
}
